//! VGM patch and MIDI extraction.
//!
//! Analyzes VGM command streams to extract:
//! - Patches (register snapshots at key-on events)
//! - Note events (frequency + timing → MIDI)

use std::collections::HashMap;

use crate::midi_file::TimedMidiEvent;
use crate::patch::Patch;
use crate::vgm::{VgmCommand, VgmFile};

/// Extracted data from a VGM file.
#[derive(Debug)]
pub struct VgmExtraction {
    pub patches: Vec<Patch>,
    pub events: Vec<TimedMidiEvent>,
    pub chip_name: String,
    pub duration_us: u64,
}

/// Extract patches and MIDI events from a VGM file.
pub fn extract(vgm: &VgmFile) -> Vec<VgmExtraction> {
    let mut results = Vec::new();

    // Detect which chips are present and extract from each
    if !vgm.chips_used.is_empty() {
        for chip in &vgm.chips_used {
            let extraction = match chip.name {
                "SN76489" => extract_sn76489(vgm),
                "YM2612" => extract_ym2612(vgm),
                "YM2151" => extract_ym2151(vgm),
                "AY-3-8910" => extract_ay8910(vgm),
                "NES APU" => extract_nes_apu(vgm),
                "POKEY" => extract_pokey(vgm),
                _ => continue,
            };
            results.push(extraction);
        }
    }

    results
}

fn samples_to_us(samples: u64) -> u64 {
    samples * 1_000_000 / 44100
}

fn freq_to_midi_note(hz: f64) -> u8 {
    if hz <= 0.0 {
        return 0;
    }
    let note = 69.0 + 12.0 * (hz / 440.0).log2();
    note.round().clamp(0.0, 127.0) as u8
}

// =============================================================================
// SN76489 extraction
// =============================================================================

fn extract_sn76489(vgm: &VgmFile) -> VgmExtraction {
    let clock = vgm.header.sn76489_clock as f64;
    let mut events = Vec::new();
    let mut patches = Vec::new();

    // Track per-channel state
    let mut freq_regs: [u16; 3] = [0; 3]; // 10-bit tone period
    let mut volumes: [u8; 3] = [15; 3]; // 15 = silent
    let mut latched_is_vol: bool = false;
    let mut latched_channel: usize = 0;
    let mut active_note: [Option<u8>; 3] = [None; 3];

    let mut seen_patches: HashMap<String, bool> = HashMap::new();

    for tc in &vgm.commands {
        if let VgmCommand::Sn76489Write { data } = &tc.command {
            let d = *data;
            if d & 0x80 != 0 {
                // Latch byte
                latched_channel = ((d >> 5) & 0x03) as usize;
                latched_is_vol = (d & 0x10) != 0;
                let low_nibble = (d & 0x0F) as u16;

                if latched_channel < 3 {
                    if latched_is_vol {
                        let old_vol = volumes[latched_channel];
                        volumes[latched_channel] = d & 0x0F;
                        let new_vol = volumes[latched_channel];

                        // Key-on: volume goes from silent to audible
                        if old_vol == 15 && new_vol < 15 && freq_regs[latched_channel] > 0 {
                            let hz = clock / (32.0 * freq_regs[latched_channel] as f64);
                            let note = freq_to_midi_note(hz);
                            events.push(TimedMidiEvent {
                                time_us: samples_to_us(tc.sample_pos),
                                note,
                                velocity: ((15 - new_vol) * 8).min(127),
                                is_on: true,
                            });
                            active_note[latched_channel] = Some(note);
                        }
                        // Key-off: volume goes to silent
                        if old_vol < 15 && new_vol == 15 {
                            if let Some(note) = active_note[latched_channel].take() {
                                events.push(TimedMidiEvent {
                                    time_us: samples_to_us(tc.sample_pos),
                                    note,
                                    velocity: 0,
                                    is_on: false,
                                });
                            }
                        }
                    } else {
                        // Frequency latch - low 4 bits
                        freq_regs[latched_channel] =
                            (freq_regs[latched_channel] & 0x3F0) | low_nibble;
                    }
                }
                // latched_reg used for noise channel (not implemented yet)
            } else {
                // Data byte - high bits of frequency
                if !latched_is_vol && latched_channel < 3 {
                    let high_bits = (d & 0x3F) as u16;
                    freq_regs[latched_channel] =
                        (freq_regs[latched_channel] & 0x00F) | (high_bits << 4);
                }
            }
        }
    }

    // Generate a patch for SN76489 (simple — just volume, since it's a square wave)
    let patch_key = "sn76489_default".to_string();
    if seen_patches.insert(patch_key, true).is_none() {
        patches.push(Patch {
            name: "SN76489 VGM".into(),
            chip: "sn76489".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 0.0),
                ("1".into(), 0.0),
                ("2".into(), 0.0),
                ("3".into(), 15.0),
            ]),
        });
    }

    VgmExtraction {
        patches,
        events,
        chip_name: "SN76489".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

// =============================================================================
// YM2612 extraction
// =============================================================================

fn extract_ym2612(vgm: &VgmFile) -> VgmExtraction {
    let clock = vgm.header.ym2612_clock as f64;
    let mut events = Vec::new();
    let mut patches = Vec::new();

    // Shadow register state for all 6 channels
    let mut regs: [[u8; 256]; 2] = [[0; 256]; 2]; // port 0 and port 1
    let mut active_note: [Option<u8>; 6] = [None; 6];
    let mut seen_patches: HashMap<String, bool> = HashMap::new();

    for tc in &vgm.commands {
        match &tc.command {
            VgmCommand::Ym2612Port0 { reg, data } => {
                regs[0][*reg as usize] = *data;
                check_ym2612_keyon(
                    *reg,
                    *data,
                    &regs,
                    clock,
                    tc.sample_pos,
                    &mut events,
                    &mut active_note,
                    &mut patches,
                    &mut seen_patches,
                );
            }
            VgmCommand::Ym2612Port1 { reg, data } => {
                regs[1][*reg as usize] = *data;
            }
            _ => {}
        }
    }

    // Release any remaining active notes
    for note in active_note.iter().flatten() {
        events.push(TimedMidiEvent {
            time_us: samples_to_us(vgm.header.total_samples as u64),
            note: *note,
            velocity: 0,
            is_on: false,
        });
    }

    VgmExtraction {
        patches,
        events,
        chip_name: "YM2612".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

#[allow(clippy::too_many_arguments)]
fn check_ym2612_keyon(
    reg: u8,
    data: u8,
    regs: &[[u8; 256]; 2],
    clock: f64,
    sample_pos: u64,
    events: &mut Vec<TimedMidiEvent>,
    active_note: &mut [Option<u8>; 6],
    patches: &mut Vec<Patch>,
    seen_patches: &mut HashMap<String, bool>,
) {
    if reg != 0x28 {
        return;
    }
    let ch = (data & 0x07) as usize;
    let ch_idx = if ch < 3 {
        ch
    } else if (4..7).contains(&ch) {
        ch - 1
    } else {
        return;
    };
    if ch_idx >= 6 {
        return;
    }
    let ops_on = data >> 4;

    if ops_on != 0 {
        // Key-on: extract frequency and patch
        let (port, base_ch) = if ch_idx < 3 {
            (0usize, ch_idx as u8)
        } else {
            (1usize, (ch_idx - 3) as u8)
        };

        let freq_msb = regs[port][(0xA4 + base_ch) as usize];
        let freq_lsb = regs[port][(0xA0 + base_ch) as usize];
        let block = (freq_msb >> 3) & 0x07;
        let fnum = ((freq_msb as u16 & 0x07) << 8) | freq_lsb as u16;
        let hz = (fnum as f64 * (1u64 << block) as f64 * clock) / (144.0 * (1u64 << 21) as f64);
        let note = freq_to_midi_note(hz);

        events.push(TimedMidiEvent {
            time_us: samples_to_us(sample_pos),
            note,
            velocity: 100,
            is_on: true,
        });
        active_note[ch_idx] = Some(note);

        // Extract patch from current register state
        let algo = regs[port][(0xB0 + base_ch) as usize] & 0x07;
        let fb = (regs[port][(0xB0 + base_ch) as usize] >> 3) & 0x07;
        let patch_key = format!("ym2612_a{}_f{}", algo, fb);

        if seen_patches.insert(patch_key.clone(), true).is_none() {
            let mut params = HashMap::new();
            params.insert("0".into(), algo as f32);
            params.insert("1".into(), fb as f32);

            let op_offsets: [u8; 4] = [0, 8, 4, 12];
            for (op, &off) in op_offsets.iter().enumerate() {
                let reg_off = (base_ch + off) as usize;
                let tl = regs[port][0x40 + reg_off] & 0x7F;
                let ar = regs[port][0x50 + reg_off] & 0x1F;
                let d1r = regs[port][0x60 + reg_off] & 0x1F;
                let d2r = regs[port][0x70 + reg_off] & 0x1F;
                let sl = (regs[port][0x80 + reg_off] >> 4) & 0x0F;
                let rr = regs[port][0x80 + reg_off] & 0x0F;
                let mul = regs[port][0x30 + reg_off] & 0x0F;
                let dt = (regs[port][0x30 + reg_off] >> 4) & 0x07;

                let base_id = 100 + op * 100;
                params.insert(format!("{}", base_id), tl as f32);
                params.insert(format!("{}", base_id + 1), ar as f32);
                params.insert(format!("{}", base_id + 2), d1r as f32);
                params.insert(format!("{}", base_id + 3), d2r as f32);
                params.insert(format!("{}", base_id + 4), sl as f32);
                params.insert(format!("{}", base_id + 5), rr as f32);
                params.insert(format!("{}", base_id + 6), mul as f32);
                params.insert(format!("{}", base_id + 7), dt as f32);
            }

            patches.push(Patch {
                name: format!("YM2612 Alg{} FB{}", algo, fb),
                chip: "ym2612".into(),
                voice_mode: "poly".into(),
                unison_detune: 0.0,
                params,
            });
        }
    } else {
        // Key-off
        if let Some(note) = active_note[ch_idx].take() {
            events.push(TimedMidiEvent {
                time_us: samples_to_us(sample_pos),
                note,
                velocity: 0,
                is_on: false,
            });
        }
    }
}

// =============================================================================
// YM2151 extraction (similar to YM2612)
// =============================================================================

fn extract_ym2151(vgm: &VgmFile) -> VgmExtraction {
    let mut events = Vec::new();
    let mut patches = Vec::new();
    let mut regs = [0u8; 256];
    let mut active_note: [Option<u8>; 8] = [None; 8];

    for tc in &vgm.commands {
        if let VgmCommand::Ym2151Write { reg, data } = &tc.command {
            regs[*reg as usize] = *data;

            // Key-on register: 0x08
            if *reg == 0x08 {
                let ch = (*data & 0x07) as usize;
                let slots = *data >> 3;

                if slots != 0 {
                    // Key-on: read KC register for pitch
                    let kc = regs[0x28 + ch];
                    let octave = (kc >> 4) & 0x07;
                    let note_code = kc & 0x0F;
                    // Approximate: OPM octave 4 = MIDI octave 4
                    let midi_note = (octave * 12 + note_code).min(127);

                    events.push(TimedMidiEvent {
                        time_us: samples_to_us(tc.sample_pos),
                        note: midi_note,
                        velocity: 100,
                        is_on: true,
                    });
                    active_note[ch] = Some(midi_note);
                } else {
                    if let Some(note) = active_note[ch].take() {
                        events.push(TimedMidiEvent {
                            time_us: samples_to_us(tc.sample_pos),
                            note,
                            velocity: 0,
                            is_on: false,
                        });
                    }
                }
            }
        }
    }

    // Basic patch extraction
    if !events.is_empty() {
        let algo = regs[0x20] & 0x07;
        let fb = (regs[0x20] >> 3) & 0x07;
        patches.push(Patch {
            name: format!("YM2151 Alg{} FB{}", algo, fb),
            chip: "ym2151".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::from([("0".into(), algo as f32), ("1".into(), fb as f32)]),
        });
    }

    VgmExtraction {
        patches,
        events,
        chip_name: "YM2151".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

// =============================================================================
// AY-3-8910 extraction
// =============================================================================

fn extract_ay8910(vgm: &VgmFile) -> VgmExtraction {
    let clock = vgm
        .chips_used
        .iter()
        .find(|c| c.name == "AY-3-8910")
        .map(|c| c.clock as f64)
        .unwrap_or(1_773_400.0);
    let mut events = Vec::new();
    let mut regs = [0u8; 16];
    let mut active_note: [Option<u8>; 3] = [None; 3];

    for tc in &vgm.commands {
        if let VgmCommand::Ay8910Write { reg, data } = &tc.command {
            let old_val = regs[*reg as usize];
            regs[*reg as usize] = *data;

            // Volume registers: R8, R9, RA (channels 0-2)
            if *reg >= 8 && *reg <= 10 {
                let ch = (*reg - 8) as usize;
                let old_vol = old_val & 0x0F;
                let new_vol = *data & 0x0F;

                if old_vol == 0 && new_vol > 0 {
                    // Key-on
                    let period_lo = regs[ch * 2] as u16;
                    let period_hi = (regs[ch * 2 + 1] & 0x0F) as u16;
                    let period = period_lo | (period_hi << 8);
                    if period > 0 {
                        let hz = clock / (16.0 * period as f64);
                        let note = freq_to_midi_note(hz);
                        events.push(TimedMidiEvent {
                            time_us: samples_to_us(tc.sample_pos),
                            note,
                            velocity: (new_vol * 8).min(127),
                            is_on: true,
                        });
                        active_note[ch] = Some(note);
                    }
                } else if old_vol > 0 && new_vol == 0 {
                    // Key-off
                    if let Some(note) = active_note[ch].take() {
                        events.push(TimedMidiEvent {
                            time_us: samples_to_us(tc.sample_pos),
                            note,
                            velocity: 0,
                            is_on: false,
                        });
                    }
                }
            }
        }
    }

    VgmExtraction {
        patches: vec![Patch {
            name: "AY-3-8910 VGM".into(),
            chip: "ay8910".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::new(),
        }],
        events,
        chip_name: "AY-3-8910".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

// =============================================================================
// NES APU extraction
// =============================================================================

fn extract_nes_apu(vgm: &VgmFile) -> VgmExtraction {
    let clock = 1_789_773.0f64;
    let mut events = Vec::new();
    let mut regs = [0u8; 32]; // 0x4000-0x401F
    let mut active_note: [Option<u8>; 4] = [None; 4]; // pulse1, pulse2, tri, noise

    for tc in &vgm.commands {
        if let VgmCommand::NesApuWrite { reg, data } = &tc.command {
            if (*reg as usize) < 32 {
                regs[*reg as usize] = *data;

                // Pulse 1: 0x4003 (length counter load = note trigger)
                // Pulse 2: 0x4007
                // Triangle: 0x400B
                // Noise: 0x400F
                let (ch, is_trigger) = match *reg {
                    0x03 => (0, true),
                    0x07 => (1, true),
                    0x0B => (2, true),
                    0x0F => (3, true),
                    _ => (0, false),
                };

                if is_trigger && ch < 4 {
                    // Release previous note
                    if let Some(note) = active_note[ch].take() {
                        events.push(TimedMidiEvent {
                            time_us: samples_to_us(tc.sample_pos),
                            note,
                            velocity: 0,
                            is_on: false,
                        });
                    }

                    // Get frequency for pulse/triangle channels
                    let note = if ch < 3 {
                        let base = ch * 4;
                        let period_lo = regs[base + 2] as u16;
                        let period_hi = (regs[base + 3] & 0x07) as u16;
                        let period = period_lo | (period_hi << 8);
                        if period > 0 {
                            let divisor = if ch == 2 { 32.0 } else { 16.0 };
                            let hz = clock / (divisor * (period as f64 + 1.0));
                            freq_to_midi_note(hz)
                        } else {
                            60
                        }
                    } else {
                        60 // noise — fixed pitch
                    };

                    events.push(TimedMidiEvent {
                        time_us: samples_to_us(tc.sample_pos),
                        note,
                        velocity: 100,
                        is_on: true,
                    });
                    active_note[ch] = Some(note);
                }
            }
        }
    }

    VgmExtraction {
        patches: vec![Patch {
            name: "NES APU VGM".into(),
            chip: "2a03".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::new(),
        }],
        events,
        chip_name: "NES APU".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

// =============================================================================
// POKEY extraction
// =============================================================================

fn extract_pokey(vgm: &VgmFile) -> VgmExtraction {
    let clock = 1_789_773.0f64;
    let mut events = Vec::new();
    let mut freq_regs = [0u8; 4]; // AUDF0-3
    let mut vol_regs = [0u8; 4]; // AUDC0-3 (volume in low nibble)
    let mut active_note: [Option<u8>; 4] = [None; 4];

    for tc in &vgm.commands {
        if let VgmCommand::PokeyWrite { reg, data } = &tc.command {
            match *reg {
                0x00 | 0x02 | 0x04 | 0x06 => {
                    // AUDF registers
                    let ch = (*reg / 2) as usize;
                    if ch < 4 {
                        freq_regs[ch] = *data;
                    }
                }
                0x01 | 0x03 | 0x05 | 0x07 => {
                    // AUDC registers (bits 3-0 = volume)
                    let ch = ((*reg - 1) / 2) as usize;
                    if ch < 4 {
                        let old_vol = vol_regs[ch] & 0x0F;
                        vol_regs[ch] = *data;
                        let new_vol = *data & 0x0F;

                        if old_vol == 0 && new_vol > 0 {
                            let divider = freq_regs[ch] as f64;
                            if divider > 0.0 {
                                let hz = clock / (28.0 * 2.0 * (divider + 1.0));
                                let note = freq_to_midi_note(hz);
                                events.push(TimedMidiEvent {
                                    time_us: samples_to_us(tc.sample_pos),
                                    note,
                                    velocity: (new_vol * 8).min(127),
                                    is_on: true,
                                });
                                active_note[ch] = Some(note);
                            }
                        } else if old_vol > 0 && new_vol == 0 {
                            if let Some(note) = active_note[ch].take() {
                                events.push(TimedMidiEvent {
                                    time_us: samples_to_us(tc.sample_pos),
                                    note,
                                    velocity: 0,
                                    is_on: false,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    VgmExtraction {
        patches: vec![Patch {
            name: "POKEY VGM".into(),
            chip: "pokey".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::new(),
        }],
        events,
        chip_name: "POKEY".into(),
        duration_us: samples_to_us(vgm.header.total_samples as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgm;

    #[test]
    fn extract_from_synthetic_sn76489() {
        let data = vgm::create_test_vgm_sn76489();
        let vgm_file = vgm::VgmFile::parse(&data).unwrap();
        let extractions = extract(&vgm_file);

        assert_eq!(extractions.len(), 1);
        let ext = &extractions[0];
        assert_eq!(ext.chip_name, "SN76489");
        assert!(!ext.patches.is_empty(), "Should extract at least one patch");
        assert!(!ext.events.is_empty(), "Should extract note events");

        // Should have note-on and note-off events for 8 notes
        let on_events: Vec<_> = ext.events.iter().filter(|e| e.is_on).collect();
        let off_events: Vec<_> = ext.events.iter().filter(|e| !e.is_on).collect();
        assert_eq!(
            on_events.len(),
            8,
            "Should have 8 note-on events for C major scale"
        );
        assert_eq!(off_events.len(), 8, "Should have 8 note-off events");

        // First note should be around C4 (MIDI 60)
        let first_note = on_events[0].note;
        assert!(
            first_note >= 58 && first_note <= 62,
            "First note should be ~C4, got MIDI {}",
            first_note
        );

        println!(
            "Extracted {} note events, {} patches",
            ext.events.len(),
            ext.patches.len()
        );
        for e in &ext.events {
            println!(
                "  {:.3}s: {} note {}",
                e.time_us as f64 / 1_000_000.0,
                if e.is_on { "ON " } else { "OFF" },
                e.note
            );
        }
    }
}
