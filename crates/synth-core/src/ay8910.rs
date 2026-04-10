use crate::chip::*;

// AY-3-8910 / YM2149 — General Instrument PSG
// Used in ZX Spectrum, MSX, Atari ST, arcade machines
// 3 square wave channels + 1 noise generator + envelope generator

const CLOCK_ZX: f64 = 1_773_400.0; // ZX Spectrum clock
const CLOCK_DIVIDER: f64 = 8.0;

const PARAM_NOISE_PERIOD: u32 = 0;
const PARAM_NOISE_ENABLE: u32 = 1;
const PARAM_ENV_SHAPE: u32 = 2;
const PARAM_ENV_PERIOD: u32 = 3;

pub fn ay8910_param_info() -> Vec<ParamInfo> {
    vec![
        ParamInfo {
            id: PARAM_NOISE_PERIOD,
            name: "Noise Period".into(),
            group: "Noise".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 31,
                default: 0,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_NOISE_ENABLE,
            name: "Noise Mix".into(),
            group: "Noise".into(),
            kind: ParamKind::Toggle { default: false },
        },
        ParamInfo {
            id: PARAM_ENV_SHAPE,
            name: "Env Shape".into(),
            group: "Envelope".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 15,
                default: 0,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_ENV_PERIOD,
            name: "Env Period".into(),
            group: "Envelope".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 65535.0,
                default: 1000.0,
            },
        },
    ]
}

#[derive(Debug, Clone)]
struct ToneChannel {
    period: u16, // 12-bit tone period
    counter: u16,
    output: bool,
    volume: u8, // 4-bit (0-15), or 16 = use envelope
}

impl ToneChannel {
    fn new() -> Self {
        ToneChannel {
            period: 0,
            counter: 0,
            output: false,
            volume: 0,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.period;
            self.output = !self.output;
        }
    }
}

// Volume table: AY-3-8910 uses logarithmic volume
fn build_volume_table() -> [f32; 16] {
    let mut table = [0.0f32; 16];
    for (i, entry) in table.iter_mut().enumerate() {
        if i == 0 {
            *entry = 0.0;
        } else {
            *entry = 10.0f32.powf((i as f32 - 15.0) * 2.0 / 20.0);
        }
    }
    table
}

pub struct Ay8910 {
    tones: [ToneChannel; 3],
    noise_period: u8,
    noise_counter: u16,
    noise_lfsr: u32,
    noise_output: bool,
    noise_enable: [bool; 3],
    env_shape: u8,
    env_period: u16,
    env_counter: u32,
    env_step: u8,
    volume_table: [f32; 16],
    #[allow(dead_code)]
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl Ay8910 {
    pub fn new(output_sample_rate: u32) -> Self {
        let native_rate = CLOCK_ZX / CLOCK_DIVIDER;
        Ay8910 {
            tones: [ToneChannel::new(), ToneChannel::new(), ToneChannel::new()],
            noise_period: 0,
            noise_counter: 0,
            noise_lfsr: 1,
            noise_output: false,
            noise_enable: [false; 3],
            env_shape: 0,
            env_period: 1000,
            env_counter: 0,
            env_step: 0,
            volume_table: build_volume_table(),
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: native_rate / output_sample_rate as f64,
        }
    }

    fn tick(&mut self) {
        for tone in &mut self.tones {
            tone.tick();
        }
        // Noise
        if self.noise_counter > 0 {
            self.noise_counter -= 1;
        } else {
            self.noise_counter = self.noise_period as u16;
            // LFSR: bit 0 XOR bit 3, shift right, feedback to bit 16
            let feedback = (self.noise_lfsr ^ (self.noise_lfsr >> 3)) & 1;
            self.noise_lfsr = (self.noise_lfsr >> 1) | (feedback << 16);
            self.noise_output = (self.noise_lfsr & 1) != 0;
        }
        // Envelope
        if self.env_counter > 0 {
            self.env_counter -= 1;
        } else {
            self.env_counter = self.env_period as u32;
            if self.env_step < 32 {
                self.env_step += 1;
            }
        }
    }

    fn mix_sample(&self) -> f32 {
        let mut out = 0.0f32;
        for (i, tone) in self.tones.iter().enumerate() {
            let tone_on = tone.output || tone.period == 0;
            let noise_on = !self.noise_enable[i] || self.noise_output;
            if tone_on && noise_on {
                let vol = tone.volume.min(15);
                out += self.volume_table[vol as usize];
            }
        }
        out / 3.0
    }
}

fn midi_note_to_period(note: u8, detune_cents: f32) -> u16 {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    let period = (CLOCK_ZX / (CLOCK_DIVIDER * 2.0 * freq)).round() as u16;
    period.clamp(1, 4095)
}

impl SoundChip for Ay8910 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ay8910
    }

    fn num_voices(&self) -> usize {
        3
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        ay8910_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_NOISE_PERIOD => self.noise_period = (value as u8).min(31),
            PARAM_NOISE_ENABLE => {
                let on = value >= 0.5;
                self.noise_enable = [on, on, on];
            }
            PARAM_ENV_SHAPE => self.env_shape = (value as u8).min(15),
            PARAM_ENV_PERIOD => self.env_period = value as u16,
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_NOISE_PERIOD => self.noise_period as f32,
            PARAM_NOISE_ENABLE => {
                if self.noise_enable[0] {
                    1.0
                } else {
                    0.0
                }
            }
            PARAM_ENV_SHAPE => self.env_shape as f32,
            PARAM_ENV_PERIOD => self.env_period as f32,
            _ => 0.0,
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 3 {
            return;
        }
        self.tones[voice].period = midi_note_to_period(note, detune_cents);
        self.tones[voice].volume = ((velocity as u16 * 15) / 127) as u8;
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 3 {
            return;
        }
        self.tones[voice].volume = 0;
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
        self.tones = [ToneChannel::new(), ToneChannel::new(), ToneChannel::new()];
        self.noise_lfsr = 1;
        self.noise_output = false;
        self.env_step = 0;
        self.phase_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_after_creation() {
        let mut chip = Ay8910::new(44100);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0));
    }

    #[test]
    fn produces_sound() {
        let mut chip = Ay8910::new(44100);
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01);
    }
}
