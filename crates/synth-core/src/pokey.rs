use crate::chip::*;

// POKEY — Atari 800/5200/7800 sound chip
// 4 channels, each with frequency divider + polynomial counter (noise)
// Unique feature: channels can be clocked at different base rates

const CLOCK_NTSC: f64 = 1_789_773.0;

const PARAM_DISTORTION: u32 = 0; // global distortion/poly counter mode

pub fn pokey_param_info() -> Vec<ParamInfo> {
    vec![ParamInfo {
        id: PARAM_DISTORTION,
        name: "Distortion".into(),
        group: "Global".into(),
        kind: ParamKind::Discrete {
            min: 0,
            max: 3,
            default: 0,
            labels: Some(vec![
                "Pure".into(),
                "Buzzy".into(),
                "Gritty".into(),
                "Metallic".into(),
            ]),
        },
    }]
}

#[derive(Debug, Clone)]
struct Channel {
    divider: u8, // 8-bit frequency divider (AUDF)
    counter: u16,
    volume: u8, // 4-bit volume (AUDV)
    output: bool,
    // Polynomial counters for noise
    poly4: u8,      // 4-bit LFSR
    poly5: u8,      // 5-bit LFSR
    poly17: u32,    // 17-bit LFSR
    distortion: u8, // AUDC distortion mode (0-3 simplified)
}

impl Channel {
    fn new() -> Self {
        Channel {
            divider: 0,
            counter: 0,
            volume: 0,
            output: false,
            poly4: 0x0F,
            poly5: 0x1F,
            poly17: 0x1FFFF,
            distortion: 0,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.divider as u16;

            match self.distortion {
                0 => {
                    // Pure tone (square wave)
                    self.output = !self.output;
                }
                1 => {
                    // 4-bit poly (buzzy)
                    let feedback = ((self.poly4 >> 3) ^ (self.poly4 >> 2)) & 1;
                    self.poly4 = ((self.poly4 << 1) | feedback) & 0x0F;
                    self.output = (self.poly4 & 1) != 0;
                }
                2 => {
                    // 5-bit poly (gritty)
                    let feedback = ((self.poly5 >> 4) ^ (self.poly5 >> 2)) & 1;
                    self.poly5 = ((self.poly5 << 1) | feedback) & 0x1F;
                    self.output = (self.poly5 & 1) != 0;
                }
                _ => {
                    // 17-bit poly (metallic noise)
                    let feedback = ((self.poly17 >> 16) ^ (self.poly17 >> 11)) & 1;
                    self.poly17 = ((self.poly17 << 1) | feedback) & 0x1FFFF;
                    self.output = (self.poly17 & 1) != 0;
                }
            }
        }
    }

    fn sample(&self) -> f32 {
        if self.output {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }
}

#[allow(dead_code)]
pub struct Pokey {
    channels: [Channel; 4],
    distortion: u8,
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl Pokey {
    pub fn new(output_sample_rate: u32) -> Self {
        // POKEY's base clock divides by 28 for the standard rate
        let native_rate = CLOCK_NTSC / 28.0;
        Pokey {
            channels: [
                Channel::new(),
                Channel::new(),
                Channel::new(),
                Channel::new(),
            ],
            distortion: 0,
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: native_rate / output_sample_rate as f64,
        }
    }

    fn tick(&mut self) {
        for ch in &mut self.channels {
            ch.tick();
        }
    }

    fn mix_sample(&self) -> f32 {
        let mut out = 0.0f32;
        for ch in &self.channels {
            out += ch.sample();
        }
        out * 0.25
    }
}

fn midi_note_to_divider(note: u8, detune_cents: f32) -> u8 {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    let divider = (CLOCK_NTSC / (28.0 * 2.0 * freq) - 1.0).round() as i32;
    divider.clamp(0, 255) as u8
}

impl SoundChip for Pokey {
    fn chip_id(&self) -> ChipId {
        ChipId::Pokey
    }

    fn num_voices(&self) -> usize {
        4
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        pokey_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        if param_id == PARAM_DISTORTION {
            self.distortion = (value as u8).min(3);
            for ch in &mut self.channels {
                ch.distortion = self.distortion;
            }
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        if param_id == PARAM_DISTORTION {
            self.distortion as f32
        } else {
            0.0
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 4 {
            return;
        }
        self.channels[voice].divider = midi_note_to_divider(note, detune_cents);
        self.channels[voice].volume = ((velocity as u16 * 15) / 127) as u8;
        self.channels[voice].distortion = self.distortion;
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 4 {
            return;
        }
        self.channels[voice].volume = 0;
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
        self.channels = [
            Channel::new(),
            Channel::new(),
            Channel::new(),
            Channel::new(),
        ];
        self.phase_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_after_creation() {
        let mut chip = Pokey::new(44100);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0));
    }

    #[test]
    fn produces_sound() {
        let mut chip = Pokey::new(44100);
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01);
    }

    #[test]
    fn four_voices() {
        let mut chip = Pokey::new(44100);
        for i in 0..4 {
            chip.voice_on(i, 60 + i as u8 * 4, 127, 0.0);
        }
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.05);
    }
}
