//! SID file extraction via 6502 emulation.
//!
//! Runs a 6502 CPU emulator to play a .sid file, logging all writes to
//! SID registers ($D400-$D41C). The logged writes are then analyzed to
//! extract patches and MIDI note events.

use std::collections::HashMap;
use std::path::Path;

use mos6502::memory::Bus;

use crate::midi_file::TimedMidiEvent;
use crate::patch::Patch;
use crate::vgm_extract::VgmExtraction;

/// SID register write captured during emulation.
#[derive(Debug, Clone, Copy)]
struct SidWrite {
    frame: u32,
    reg: u8, // 0x00-0x1C (relative to $D400)
    value: u8,
}

/// Parse a SID file header.
#[allow(dead_code)]
struct SidHeader {
    version: u16,
    header_len: u16,
    load_addr: u16,
    init_addr: u16,
    play_addr: u16,
    songs: u16,
    default_song: u16,
    name: String,
    author: String,
    data: Vec<u8>, // program data after header
}

impl SidHeader {
    fn parse(raw: &[u8]) -> anyhow::Result<Self> {
        if raw.len() < 0x76 {
            anyhow::bail!("SID file too short");
        }
        let magic = &raw[0..4];
        if magic != b"PSID" && magic != b"RSID" {
            anyhow::bail!("Not a SID file (bad magic)");
        }

        let version = u16::from_be_bytes([raw[4], raw[5]]);
        let header_len = u16::from_be_bytes([raw[6], raw[7]]);
        let load_addr = u16::from_be_bytes([raw[8], raw[9]]);
        let init_addr = u16::from_be_bytes([raw[10], raw[11]]);
        let play_addr = u16::from_be_bytes([raw[12], raw[13]]);
        let songs = u16::from_be_bytes([raw[14], raw[15]]);
        let default_song = u16::from_be_bytes([raw[16], raw[17]]);

        let name = String::from_utf8_lossy(&raw[0x16..0x36])
            .trim_end_matches('\0')
            .to_string();
        let author = String::from_utf8_lossy(&raw[0x36..0x56])
            .trim_end_matches('\0')
            .to_string();

        let data_start = header_len as usize;
        let data = if data_start < raw.len() {
            raw[data_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(SidHeader {
            version,
            header_len,
            load_addr,
            init_addr,
            play_addr,
            songs,
            default_song,
            name,
            author,
            data,
        })
    }
}

/// Run 6502 emulation on a SID file and capture register writes.
fn emulate_sid(header: &SidHeader, song: u16, frames: u32) -> Vec<SidWrite> {
    use mos6502::cpu;
    use mos6502::instruction::Nmos6502;
    use mos6502::registers::StackPointer;

    // Set up memory with SID register capture
    let mut memory = [0u8; 65536];

    // Load the program data into memory
    let load_addr = if header.load_addr == 0 {
        if header.data.len() >= 2 {
            u16::from_le_bytes([header.data[0], header.data[1]])
        } else {
            return Vec::new();
        }
    } else {
        header.load_addr
    };

    let data_offset = if header.load_addr == 0 { 2 } else { 0 };
    let data_to_load = &header.data[data_offset..];
    let load_end = (load_addr as usize + data_to_load.len()).min(65536);
    memory[load_addr as usize..load_end]
        .copy_from_slice(&data_to_load[..load_end - load_addr as usize]);

    // RTS trap at $FFF0
    memory[0xFFF0] = 0x60; // RTS

    // IRQ/NMI/RESET vectors → RTS trap
    memory[0xFFFA] = 0xF0;
    memory[0xFFFB] = 0xFF;
    memory[0xFFFC] = 0xF0;
    memory[0xFFFD] = 0xFF;
    memory[0xFFFE] = 0xF0;
    memory[0xFFFF] = 0xFF;

    // Create CPU with NMOS 6502 variant
    let mut cpu = cpu::CPU::new(
        MemBus {
            mem: memory,
            sid_writes: Vec::new(),
            current_frame: 0,
        },
        Nmos6502,
    );

    // Set up registers
    cpu.registers.stack_pointer = StackPointer(0xFF);
    cpu.registers.accumulator = song.saturating_sub(1) as u8;
    cpu.registers.index_x = 0;
    cpu.registers.index_y = 0;

    // Push return address for RTS ($FFF0 - 1 = $FFEF, because RTS adds 1)
    let ret_hi = 0xFFu8;
    let ret_lo = 0xEFu8;
    cpu.memory.mem[0x01FF] = ret_hi;
    cpu.memory.mem[0x01FE] = ret_lo;
    cpu.registers.stack_pointer = StackPointer(0xFD);

    // Execute init
    cpu.registers.program_counter = header.init_addr;
    for _ in 0..100_000 {
        if cpu.registers.program_counter == 0xFFF0 {
            break;
        }
        cpu.single_step();
    }

    // Call play routine every frame
    for frame in 0..frames {
        cpu.memory.current_frame = frame;

        // Push return address onto stack
        let sp = cpu.registers.stack_pointer.0;
        cpu.memory.mem[0x0100 + sp as usize] = ret_hi;
        cpu.memory.mem[0x0100 + sp.wrapping_sub(1) as usize] = ret_lo;
        cpu.registers.stack_pointer = StackPointer(sp.wrapping_sub(2));

        cpu.registers.program_counter = header.play_addr;

        for _ in 0..50_000 {
            if cpu.registers.program_counter == 0xFFF0 {
                break;
            }
            cpu.single_step();
        }
    }

    cpu.memory.sid_writes
}

/// Memory bus that intercepts SID register writes.
struct MemBus {
    mem: [u8; 65536],
    sid_writes: Vec<SidWrite>,
    current_frame: u32,
}

impl Bus for MemBus {
    fn get_byte(&mut self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }

    fn set_byte(&mut self, addr: u16, val: u8) {
        self.mem[addr as usize] = val;

        // Intercept SID register writes ($D400-$D41C)
        if (0xD400..=0xD41C).contains(&addr) {
            self.sid_writes.push(SidWrite {
                frame: self.current_frame,
                reg: (addr - 0xD400) as u8,
                value: val,
            });
        }
    }
}

/// Extract patches and MIDI from a .sid file.
pub fn extract_sid_file(path: &Path) -> anyhow::Result<(VgmExtraction, SidInfo)> {
    let raw = std::fs::read(path)?;
    let header = SidHeader::parse(&raw)?;

    let info = SidInfo {
        name: header.name.clone(),
        author: header.author.clone(),
        songs: header.songs,
        default_song: header.default_song,
    };

    log::info!(
        "SID: '{}' by {} ({} songs, init=${:04X} play=${:04X})",
        header.name,
        header.author,
        header.songs,
        header.init_addr,
        header.play_addr
    );

    // Emulate for ~60 seconds at 50 Hz (PAL)
    let frames = 50 * 60;
    let writes = emulate_sid(&header, header.default_song, frames);
    log::info!(
        "Captured {} SID register writes in {} frames",
        writes.len(),
        frames
    );

    // Convert register writes to note events
    let extraction = analyze_sid_writes(&writes, &header.name);

    Ok((extraction, info))
}

#[derive(Debug)]
pub struct SidInfo {
    pub name: String,
    pub author: String,
    pub songs: u16,
    pub default_song: u16,
}

fn analyze_sid_writes(writes: &[SidWrite], name: &str) -> VgmExtraction {
    let mut events = Vec::new();
    let frame_us = 20_000u64; // 50 Hz PAL = 20ms per frame

    // Track state of 3 SID voices
    // Voice 0: regs 0x00-0x06
    // Voice 1: regs 0x07-0x0D
    // Voice 2: regs 0x0E-0x14
    let mut freq_lo = [0u8; 3];
    let mut freq_hi = [0u8; 3];
    let mut control = [0u8; 3]; // control register (waveform + gate)
    let mut active_note: [Option<u8>; 3] = [None; 3];

    // Track unique patches
    let mut ad = [0u8; 3]; // attack/decay
    let mut sr = [0u8; 3]; // sustain/release
    let mut pw_lo = [0u8; 3];
    let mut pw_hi = [0u8; 3];

    for w in writes {
        let voice = (w.reg / 7) as usize;
        let local_reg = w.reg % 7;

        if voice >= 3 {
            continue; // Filter/volume registers, skip for note extraction
        }

        match local_reg {
            0 => freq_lo[voice] = w.value,
            1 => freq_hi[voice] = w.value,
            2 => pw_lo[voice] = w.value,
            3 => pw_hi[voice] = w.value & 0x0F,
            4 => {
                let old_gate = control[voice] & 1;
                let new_gate = w.value & 1;
                control[voice] = w.value;

                if old_gate == 0 && new_gate == 1 {
                    // Gate on — note on
                    let freq = (freq_hi[voice] as u16) << 8 | freq_lo[voice] as u16;
                    let hz = freq as f64 * 985248.0 / 16777216.0;
                    let note = if hz > 0.0 {
                        (69.0 + 12.0 * (hz / 440.0).log2())
                            .round()
                            .clamp(0.0, 127.0) as u8
                    } else {
                        60
                    };

                    events.push(TimedMidiEvent {
                        time_us: w.frame as u64 * frame_us,
                        note,
                        velocity: 100,
                        is_on: true,
                    });
                    active_note[voice] = Some(note);
                } else if old_gate == 1 && new_gate == 0 {
                    // Gate off — note off
                    if let Some(note) = active_note[voice].take() {
                        events.push(TimedMidiEvent {
                            time_us: w.frame as u64 * frame_us,
                            note,
                            velocity: 0,
                            is_on: false,
                        });
                    }
                }
            }
            5 => ad[voice] = w.value,
            6 => sr[voice] = w.value,
            _ => {}
        }
    }

    // Extract patch from the most common waveform/ADSR settings
    let waveform = (control[0] >> 4) & 0x0F;
    let waveform_name = match waveform {
        1 => "Triangle",
        2 => "Sawtooth",
        4 => "Pulse",
        8 => "Noise",
        _ => "Mixed",
    };

    let patches = vec![Patch {
        name: format!("{} ({})", name, waveform_name),
        chip: "sid6581".into(),
        voice_mode: "poly".into(),
        unison_detune: 0.0,
        params: HashMap::from([
            (
                "0".into(),
                match waveform {
                    1 => 0.0,
                    2 => 1.0,
                    4 => 2.0,
                    8 => 3.0,
                    _ => 1.0,
                },
            ),
            (
                "1".into(),
                ((pw_hi[0] as u16) << 8 | pw_lo[0] as u16) as f32,
            ),
            ("2".into(), (ad[0] >> 4) as f32),   // attack
            ("3".into(), (ad[0] & 0x0F) as f32), // decay
            ("4".into(), (sr[0] >> 4) as f32),   // sustain
            ("5".into(), (sr[0] & 0x0F) as f32), // release
            ("6".into(), 15.0),                  // volume
        ]),
    }];

    events.sort_by_key(|e| e.time_us);

    let duration_us = events.last().map(|e| e.time_us).unwrap_or(0);

    VgmExtraction {
        patches,
        events,
        chip_name: "SID 6581".into(),
        duration_us,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sid_header() {
        // Check if our test SID file exists
        let path = Path::new("/tmp/commando.sid");
        if !path.exists() {
            eprintln!("Skipping: /tmp/commando.sid not found");
            return;
        }
        let raw = std::fs::read(path).unwrap();
        let header = SidHeader::parse(&raw).unwrap();
        assert_eq!(header.name, "Commando");
        assert_eq!(header.author, "Rob Hubbard");
        assert_eq!(header.songs, 19);
        assert!(header.init_addr > 0);
        assert!(header.play_addr > 0);
    }

    #[test]
    fn extract_commando_sid() {
        let path = Path::new("/tmp/commando.sid");
        if !path.exists() {
            eprintln!("Skipping: /tmp/commando.sid not found");
            return;
        }
        let (extraction, info) = extract_sid_file(path).unwrap();
        println!("SID: {} by {}", info.name, info.author);
        println!(
            "Extracted: {} events, {} patches",
            extraction.events.len(),
            extraction.patches.len()
        );
        let on_count = extraction.events.iter().filter(|e| e.is_on).count();
        println!("Note-on events: {}", on_count);
        assert!(on_count > 0, "Should extract note events from Commando");

        // Print first 20 events
        for e in extraction.events.iter().take(20) {
            println!(
                "  {:.3}s: {} note {}",
                e.time_us as f64 / 1_000_000.0,
                if e.is_on { "ON " } else { "OFF" },
                e.note
            );
        }
    }
}
