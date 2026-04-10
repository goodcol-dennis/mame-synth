use crate::chip::*;

const CLOCK_NTSC: u32 = 3_579_545;
const CLOCK_DIVIDER: u32 = 16;

// Volume attenuation table: 2dB per step, index 15 = silence
fn build_volume_table() -> [f32; 16] {
    let mut table = [0.0f32; 16];
    for (i, entry) in table[..15].iter_mut().enumerate() {
        // Each step is -2dB; 0 = full volume
        *entry = 10.0f32.powf(-2.0 * i as f32 / 20.0);
    }
    table[15] = 0.0; // silence
    table
}

// Parameter IDs
const PARAM_TONE1_VOL: u32 = 0;
const PARAM_TONE2_VOL: u32 = 1;
const PARAM_TONE3_VOL: u32 = 2;
const PARAM_NOISE_VOL: u32 = 3;
const PARAM_NOISE_MODE: u32 = 4;
const PARAM_NOISE_RATE: u32 = 5;

pub fn sn76489_param_info() -> Vec<ParamInfo> {
    vec![
        ParamInfo {
            id: PARAM_TONE1_VOL,
            name: "Volume".into(),
            group: "Tone 1".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 15.0,
                default: 0.0,
            },
        },
        ParamInfo {
            id: PARAM_TONE2_VOL,
            name: "Volume".into(),
            group: "Tone 2".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 15.0,
                default: 0.0,
            },
        },
        ParamInfo {
            id: PARAM_TONE3_VOL,
            name: "Volume".into(),
            group: "Tone 3".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 15.0,
                default: 0.0,
            },
        },
        ParamInfo {
            id: PARAM_NOISE_VOL,
            name: "Volume".into(),
            group: "Noise".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 15.0,
                default: 15.0,
            },
        },
        ParamInfo {
            id: PARAM_NOISE_MODE,
            name: "White Noise".into(),
            group: "Noise".into(),
            kind: ParamKind::Toggle { default: true },
        },
        ParamInfo {
            id: PARAM_NOISE_RATE,
            name: "Shift Rate".into(),
            group: "Noise".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 3,
                default: 0,
                labels: Some(vec![
                    "N/512".into(),
                    "N/1024".into(),
                    "N/2048".into(),
                    "Tone 3".into(),
                ]),
            },
        },
    ]
}

#[derive(Debug, Clone)]
struct ToneChannel {
    frequency: u16, // 10-bit frequency register (0-1023)
    volume: u8,     // 4-bit attenuation (0=loud, 15=silent)
    counter: u16,
    output: bool,
}

impl ToneChannel {
    fn new() -> Self {
        ToneChannel {
            frequency: 0,
            volume: 15, // silent
            counter: 0,
            output: false,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.frequency;
            self.output = !self.output;
        }
    }
}

#[derive(Debug, Clone)]
struct NoiseChannel {
    volume: u8,
    shift_rate: u8,    // 0-3
    white_noise: bool, // true = white, false = periodic
    lfsr: u16,
    counter: u16,
    output: bool,
}

impl NoiseChannel {
    fn new() -> Self {
        NoiseChannel {
            volume: 15,
            shift_rate: 0,
            white_noise: true,
            lfsr: 0x8000,
            counter: 0,
            output: false,
        }
    }

    fn shift_value(&self) -> u16 {
        match self.shift_rate {
            0 => 0x10, // N/512
            1 => 0x20, // N/1024
            2 => 0x40, // N/2048
            _ => 0,    // Tone 3 frequency (handled externally)
        }
    }

    fn tick(&mut self, tone3_freq: u16) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            let period = if self.shift_rate == 3 {
                tone3_freq
            } else {
                self.shift_value()
            };
            self.counter = period;

            // Clock the LFSR
            let feedback = if self.white_noise {
                // White noise: XOR of bits 0 and 3
                (self.lfsr & 1) ^ ((self.lfsr >> 3) & 1)
            } else {
                // Periodic: bit 0 only
                self.lfsr & 1
            };
            self.output = (self.lfsr & 1) != 0;
            self.lfsr = (self.lfsr >> 1) | (feedback << 15);
        }
    }
}

#[allow(dead_code)]
pub struct Sn76489 {
    tones: [ToneChannel; 3],
    noise: NoiseChannel,
    volume_table: [f32; 16],
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl Sn76489 {
    pub fn new(output_sample_rate: u32) -> Self {
        let native_rate = CLOCK_NTSC as f64 / CLOCK_DIVIDER as f64;
        Sn76489 {
            tones: [ToneChannel::new(), ToneChannel::new(), ToneChannel::new()],
            noise: NoiseChannel::new(),
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
        self.noise.tick(self.tones[2].frequency);
    }

    fn mix_sample(&self) -> f32 {
        let mut out = 0.0f32;
        for tone in &self.tones {
            if tone.output {
                out += self.volume_table[tone.volume as usize];
            }
        }
        if self.noise.output {
            out += self.volume_table[self.noise.volume as usize];
        }
        // Mix: scale up for audible output
        out * 0.5
    }
}

impl SoundChip for Sn76489 {
    fn chip_id(&self) -> ChipId {
        ChipId::Sn76489
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        sn76489_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_TONE1_VOL => self.tones[0].volume = (value as u8).min(15),
            PARAM_TONE2_VOL => self.tones[1].volume = (value as u8).min(15),
            PARAM_TONE3_VOL => self.tones[2].volume = (value as u8).min(15),
            PARAM_NOISE_VOL => self.noise.volume = (value as u8).min(15),
            PARAM_NOISE_MODE => self.noise.white_noise = value >= 0.5,
            PARAM_NOISE_RATE => self.noise.shift_rate = (value as u8).min(3),
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_TONE1_VOL => self.tones[0].volume as f32,
            PARAM_TONE2_VOL => self.tones[1].volume as f32,
            PARAM_TONE3_VOL => self.tones[2].volume as f32,
            PARAM_NOISE_VOL => self.noise.volume as f32,
            PARAM_NOISE_MODE => {
                if self.noise.white_noise {
                    1.0
                } else {
                    0.0
                }
            }
            PARAM_NOISE_RATE => self.noise.shift_rate as f32,
            _ => 0.0,
        }
    }

    fn num_voices(&self) -> usize {
        3
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 3 {
            return;
        }
        let freq_hz =
            440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
        let n = (CLOCK_NTSC as f64 / (2.0 * CLOCK_DIVIDER as f64 * freq_hz)).round() as u16;
        let freq_reg = n.clamp(1, 1023);
        let attenuation = 15 - ((velocity as u16 * 15) / 127) as u8;
        self.tones[voice].frequency = freq_reg;
        self.tones[voice].volume = attenuation;
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 3 {
            return;
        }
        self.tones[voice].volume = 15;
    }

    fn generate_samples(&mut self, output: &mut [StereoSample]) {
        for sample in output.iter_mut() {
            // Run the chip at its native rate, output at the audio rate
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
        for tone in &mut self.tones {
            *tone = ToneChannel::new();
        }
        self.noise = NoiseChannel::new();
        self.phase_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chip() -> Sn76489 {
        Sn76489::new(44100)
    }

    #[test]
    fn silent_after_creation() {
        let mut chip = make_chip();
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0 && s.right == 0.0));
    }

    #[test]
    fn produces_sound_after_voice_on() {
        let mut chip = make_chip();
        chip.voice_on(0, 60, 127, 0.0); // middle C, max velocity
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01, "Expected audible output, got peak={}", peak);
    }

    #[test]
    fn silent_after_voice_off() {
        let mut chip = make_chip();
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        chip.voice_off(0);
        // Generate more samples — should be silent
        let mut buf2 = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf2);
        assert!(buf2.iter().all(|s| s.left == 0.0 && s.right == 0.0));
    }

    #[test]
    fn reset_silences_output() {
        let mut chip = make_chip();
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        chip.reset();
        let mut buf2 = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf2);
        assert!(buf2.iter().all(|s| s.left == 0.0 && s.right == 0.0));
    }

    #[test]
    fn three_voices_independent() {
        let mut chip = make_chip();
        chip.voice_on(0, 60, 127, 0.0);
        chip.voice_on(1, 64, 127, 0.0);
        chip.voice_on(2, 67, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 512];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.05, "Three voices should be louder");
    }

    #[test]
    fn detune_changes_frequency() {
        let mut chip1 = make_chip();
        let mut chip2 = make_chip();
        chip1.voice_on(0, 60, 127, 0.0);
        chip2.voice_on(0, 60, 127, 50.0); // 50 cents sharp
        // They should produce different sample patterns
        let mut buf1 = vec![StereoSample::default(); 512];
        let mut buf2 = vec![StereoSample::default(); 512];
        chip1.generate_samples(&mut buf1);
        chip2.generate_samples(&mut buf2);
        let differs = buf1.iter().zip(buf2.iter()).any(|(a, b)| a.left != b.left);
        assert!(differs, "Detuned voice should produce different samples");
    }

    #[test]
    fn volume_table_correct() {
        let table = build_volume_table();
        assert_eq!(table[15], 0.0); // silence
        assert!(table[0] > table[1]); // decreasing
        assert!(table[14] > 0.0); // still audible at step 14
    }
}
