use crate::chip::*;

// MOS 6581 SID - Sound Interface Device (Commodore 64)
// 3 voices, each with: oscillator (tri/saw/pulse/noise), ADSR envelope
// Simplified emulation: no filter, no ring mod/sync for v1

const CLOCK_PAL: f64 = 985_248.0;

// ADSR rate table: maps 4-bit rate value to cycles per step
// Based on the real SID's RC charge/discharge timing
const ATTACK_RATES: [u32; 16] = [
    2, 8, 16, 24, 38, 56, 68, 80, 100, 250, 500, 800, 1000, 3000, 5000, 8000,
];
const DECAY_RELEASE_RATES: [u32; 16] = [
    6, 24, 48, 72, 114, 168, 204, 240, 300, 750, 1500, 2400, 3000, 9000, 15000, 24000,
];

// Parameter IDs
const PARAM_WAVEFORM: u32 = 0;
const PARAM_PULSE_WIDTH: u32 = 1;
const PARAM_ATTACK: u32 = 2;
const PARAM_DECAY: u32 = 3;
const PARAM_SUSTAIN: u32 = 4;
const PARAM_RELEASE: u32 = 5;
const PARAM_VOLUME: u32 = 6;

pub fn sid_param_info() -> Vec<ParamInfo> {
    vec![
        ParamInfo {
            id: PARAM_WAVEFORM,
            name: "Waveform".into(),
            group: "Oscillator".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 3,
                default: 1,
                labels: Some(vec![
                    "Triangle".into(),
                    "Sawtooth".into(),
                    "Pulse".into(),
                    "Noise".into(),
                ]),
            },
        },
        ParamInfo {
            id: PARAM_PULSE_WIDTH,
            name: "Pulse Width".into(),
            group: "Oscillator".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 4095.0,
                default: 2048.0,
            },
        },
        ParamInfo {
            id: PARAM_ATTACK,
            name: "Attack".into(),
            group: "Envelope".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 2,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_DECAY,
            name: "Decay".into(),
            group: "Envelope".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 4,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_SUSTAIN,
            name: "Sustain".into(),
            group: "Envelope".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 10,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_RELEASE,
            name: "Release".into(),
            group: "Envelope".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 4,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_VOLUME,
            name: "Volume".into(),
            group: "Master".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 15,
                labels: None,
            },
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Waveform {
    Triangle,
    Sawtooth,
    Pulse,
    Noise,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EnvState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone)]
struct Voice {
    // Oscillator
    frequency: u16,     // 16-bit frequency register
    waveform: Waveform,
    pulse_width: u16,   // 12-bit pulse width (0-4095)
    accumulator: u32,   // 24-bit phase accumulator
    // Noise LFSR (23-bit)
    noise_lfsr: u32,
    noise_output: u8,
    // ADSR envelope
    attack: u8,         // 0-15
    decay: u8,          // 0-15
    sustain: u8,        // 0-15 (maps to sustain level)
    release: u8,        // 0-15
    env_state: EnvState,
    env_counter: u32,   // countdown to next envelope step
    env_level: u8,      // current envelope level 0-255
    gate: bool,         // key on/off
    // MIDI tracking
    midi_note: Option<u8>,
}

impl Voice {
    fn new() -> Self {
        Voice {
            frequency: 0,
            waveform: Waveform::Sawtooth,
            pulse_width: 2048,
            accumulator: 0,
            noise_lfsr: 0x7FFFF8,
            noise_output: 0,
            attack: 2,
            decay: 4,
            sustain: 10,
            release: 4,
            env_state: EnvState::Idle,
            env_counter: 0,
            env_level: 0,
            gate: false,
            midi_note: None,
        }
    }

    fn clock(&mut self) {
        let prev_msb = self.accumulator & 0x800000;
        self.accumulator = (self.accumulator + self.frequency as u32) & 0xFFFFFF;

        // Clock noise LFSR on bit 19 transition
        if (self.accumulator & 0x080000) != 0 && (prev_msb & 0x080000) == 0 {
            // Feedback: bit 22 XOR bit 17
            let bit22 = (self.noise_lfsr >> 22) & 1;
            let bit17 = (self.noise_lfsr >> 17) & 1;
            let feedback = bit22 ^ bit17;
            self.noise_lfsr = ((self.noise_lfsr << 1) | feedback) & 0x7FFFFF;
            // Output is bits 22,20,16,13,11,7,4,2
            self.noise_output = (
                ((self.noise_lfsr >> 22) & 1) << 7 |
                ((self.noise_lfsr >> 20) & 1) << 6 |
                ((self.noise_lfsr >> 16) & 1) << 5 |
                ((self.noise_lfsr >> 13) & 1) << 4 |
                ((self.noise_lfsr >> 11) & 1) << 3 |
                ((self.noise_lfsr >> 7) & 1) << 2 |
                ((self.noise_lfsr >> 4) & 1) << 1 |
                ((self.noise_lfsr >> 2) & 1)
            ) as u8;
        }

        // Clock envelope
        self.clock_envelope();
    }

    fn clock_envelope(&mut self) {
        if self.env_counter > 0 {
            self.env_counter -= 1;
            return;
        }

        match self.env_state {
            EnvState::Attack => {
                self.env_counter = ATTACK_RATES[self.attack as usize];
                if self.env_level < 255 {
                    self.env_level += 1;
                    if self.env_level >= 255 {
                        self.env_level = 255;
                        self.env_state = EnvState::Decay;
                        self.env_counter = DECAY_RELEASE_RATES[self.decay as usize];
                    }
                }
            }
            EnvState::Decay => {
                self.env_counter = DECAY_RELEASE_RATES[self.decay as usize];
                let sustain_level = self.sustain as u8 * 17; // 0-15 -> 0-255
                if self.env_level > sustain_level {
                    self.env_level -= 1;
                    if self.env_level <= sustain_level {
                        self.env_level = sustain_level;
                        self.env_state = EnvState::Sustain;
                    }
                }
            }
            EnvState::Sustain => {
                // Hold at sustain level while gate is on
                let sustain_level = self.sustain as u8 * 17;
                self.env_level = sustain_level;
            }
            EnvState::Release => {
                self.env_counter = DECAY_RELEASE_RATES[self.release as usize];
                if self.env_level > 0 {
                    self.env_level -= 1;
                } else {
                    self.env_state = EnvState::Idle;
                }
            }
            EnvState::Idle => {}
        }
    }

    fn gate_on(&mut self) {
        self.gate = true;
        self.env_state = EnvState::Attack;
        self.env_counter = ATTACK_RATES[self.attack as usize];
    }

    fn gate_off(&mut self) {
        self.gate = false;
        self.env_state = EnvState::Release;
        self.env_counter = DECAY_RELEASE_RATES[self.release as usize];
    }

    fn output(&self) -> f32 {
        if self.env_level == 0 {
            return 0.0;
        }

        // Get oscillator output as 12-bit unsigned (0-4095)
        let osc_top = (self.accumulator >> 12) as u16; // top 12 bits

        let osc_output: f32 = match self.waveform {
            Waveform::Triangle => {
                // Triangle: ramp up first half, ramp down second half
                let msb = (self.accumulator >> 23) & 1;
                let val = if msb == 0 {
                    (self.accumulator >> 11) & 0xFFF
                } else {
                    0xFFF - ((self.accumulator >> 11) & 0xFFF)
                };
                (val as f32 / 2048.0) - 1.0 // normalize to [-1, 1]
            }
            Waveform::Sawtooth => {
                // Sawtooth: linear ramp
                (osc_top as f32 / 2048.0) - 1.0
            }
            Waveform::Pulse => {
                // Pulse: compare accumulator top 12 bits against pulse width
                if osc_top >= self.pulse_width {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Noise => {
                // Noise: use LFSR output
                (self.noise_output as f32 / 128.0) - 1.0
            }
        };

        // Apply envelope
        let envelope = self.env_level as f32 / 255.0;
        osc_output * envelope
    }
}

#[allow(dead_code)]
pub struct Sid6581 {
    voices: [Voice; 3],
    master_volume: u8, // 0-15
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
    // Shared patch: all voices use same waveform/ADSR settings
    waveform: Waveform,
    pulse_width: u16,
    attack: u8,
    decay: u8,
    sustain: u8,
    release: u8,
}

impl Sid6581 {
    pub fn new(output_sample_rate: u32) -> Self {
        Sid6581 {
            voices: [Voice::new(), Voice::new(), Voice::new()],
            master_volume: 15,
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: CLOCK_PAL / output_sample_rate as f64,
            waveform: Waveform::Sawtooth,
            pulse_width: 2048,
            attack: 2,
            decay: 4,
            sustain: 10,
            release: 4,
        }
    }

    fn tick(&mut self) {
        for voice in &mut self.voices {
            voice.clock();
        }
    }

    fn mix_sample(&self) -> f32 {
        let mut out = 0.0f32;
        for voice in &self.voices {
            out += voice.output();
        }
        // Apply master volume and normalize
        out * (self.master_volume as f32 / 15.0) * 0.4
    }

    fn apply_patch(&mut self, voice_idx: usize) {
        self.voices[voice_idx].waveform = self.waveform;
        self.voices[voice_idx].pulse_width = self.pulse_width;
        self.voices[voice_idx].attack = self.attack;
        self.voices[voice_idx].decay = self.decay;
        self.voices[voice_idx].sustain = self.sustain;
        self.voices[voice_idx].release = self.release;
    }
}


impl SoundChip for Sid6581 {
    fn chip_id(&self) -> ChipId {
        ChipId::Sid6581
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        sid_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_WAVEFORM => {
                self.waveform = match value as u8 {
                    0 => Waveform::Triangle,
                    1 => Waveform::Sawtooth,
                    2 => Waveform::Pulse,
                    _ => Waveform::Noise,
                };
                for voice in &mut self.voices {
                    voice.waveform = self.waveform;
                }
            }
            PARAM_PULSE_WIDTH => {
                self.pulse_width = (value as u16).min(4095);
                for voice in &mut self.voices {
                    voice.pulse_width = self.pulse_width;
                }
            }
            PARAM_ATTACK => {
                self.attack = (value as u8).min(15);
                for voice in &mut self.voices {
                    voice.attack = self.attack;
                }
            }
            PARAM_DECAY => {
                self.decay = (value as u8).min(15);
                for voice in &mut self.voices {
                    voice.decay = self.decay;
                }
            }
            PARAM_SUSTAIN => {
                self.sustain = (value as u8).min(15);
                for voice in &mut self.voices {
                    voice.sustain = self.sustain;
                }
            }
            PARAM_RELEASE => {
                self.release = (value as u8).min(15);
                for voice in &mut self.voices {
                    voice.release = self.release;
                }
            }
            PARAM_VOLUME => {
                self.master_volume = (value as u8).min(15);
            }
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_WAVEFORM => match self.waveform {
                Waveform::Triangle => 0.0,
                Waveform::Sawtooth => 1.0,
                Waveform::Pulse => 2.0,
                Waveform::Noise => 3.0,
            },
            PARAM_PULSE_WIDTH => self.pulse_width as f32,
            PARAM_ATTACK => self.attack as f32,
            PARAM_DECAY => self.decay as f32,
            PARAM_SUSTAIN => self.sustain as f32,
            PARAM_RELEASE => self.release as f32,
            PARAM_VOLUME => self.master_volume as f32,
            _ => 0.0,
        }
    }

    fn num_voices(&self) -> usize {
        3
    }

    fn voice_on(&mut self, voice: usize, note: u8, _velocity: u8, detune_cents: f32) {
        if voice >= 3 { return; }
        let freq_hz = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
        let freq = ((freq_hz * 16777216.0 / CLOCK_PAL).round() as u32).min(0xFFFF) as u16;
        self.apply_patch(voice);
        self.voices[voice].frequency = freq;
        self.voices[voice].midi_note = Some(note);
        self.voices[voice].gate_on();
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 3 { return; }
        self.voices[voice].gate_off();
        self.voices[voice].midi_note = None;
    }

    fn generate_samples(&mut self, output: &mut [StereoSample]) {
        for sample in output.iter_mut() {
            self.phase_accumulator += self.phase_increment;
            while self.phase_accumulator >= 1.0 {
                self.tick();
                self.phase_accumulator -= 1.0;
            }
            let s = self.mix_sample();
            sample.left = s;
            sample.right = s;
        }
    }

    fn reset(&mut self) {
        self.voices = [Voice::new(), Voice::new(), Voice::new()];
        self.phase_accumulator = 0.0;
    }
}
