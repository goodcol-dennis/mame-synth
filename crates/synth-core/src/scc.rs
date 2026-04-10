use crate::chip::*;

// Konami SCC (Sound Creative Chip)
// 5 wavetable channels, each with a 32-byte waveform
// Used in MSX cartridges (Gradius, Salamander, Snatcher)

const CLOCK: f64 = 3_579_545.0;

const PARAM_WAVEFORM: u32 = 0;

// Preset waveforms
const WAVE_SINE: [i8; 32] = [
    0, 24, 46, 64, 77, 86, 89, 86, 77, 64, 46, 24, 0, -24, -46, -64, -77, -86, -89, -86, -77, -64,
    -46, -24, 0, 0, 0, 0, 0, 0, 0, 0,
];
const WAVE_SQUARE: [i8; 32] = [
    127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, 127, -128, -128,
    -128, -128, -128, -128, -128, -128, -128, -128, -128, -128, -128, -128, -128, -128,
];
const WAVE_SAW: [i8; 32] = [
    -128, -120, -112, -104, -96, -88, -80, -72, -64, -56, -48, -40, -32, -24, -16, -8, 0, 8, 16,
    24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120,
];
const WAVE_ORGAN: [i8; 32] = [
    0, 48, 77, 89, 86, 64, 24, -24, -64, -86, -89, -77, -48, 0, 48, 77, 89, 86, 64, 24, -24, -64,
    -86, -89, -77, -48, 0, 24, 46, 46, 24, 0,
];

pub fn scc_param_info() -> Vec<ParamInfo> {
    vec![ParamInfo {
        id: PARAM_WAVEFORM,
        name: "Waveform".into(),
        group: "Wavetable".into(),
        kind: ParamKind::Discrete {
            min: 0,
            max: 3,
            default: 0,
            labels: Some(vec![
                "Sine".into(),
                "Square".into(),
                "Saw".into(),
                "Organ".into(),
            ]),
        },
    }]
}

#[derive(Debug, Clone)]
struct Channel {
    period: u16, // 12-bit period
    counter: u16,
    wave_pos: usize, // 0-31
    volume: u8,      // 4-bit
    waveform: [i8; 32],
}

impl Channel {
    fn new() -> Self {
        Channel {
            period: 0,
            counter: 0,
            wave_pos: 0,
            volume: 0,
            waveform: WAVE_SINE,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.period;
            self.wave_pos = (self.wave_pos + 1) % 32;
        }
    }

    fn output(&self) -> f32 {
        if self.volume == 0 || self.period == 0 {
            return 0.0;
        }
        let sample = self.waveform[self.wave_pos] as f32 / 128.0;
        sample * (self.volume as f32 / 15.0)
    }
}

pub struct Scc {
    channels: [Channel; 5],
    waveform_idx: u8,
    #[allow(dead_code)]
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl Scc {
    pub fn new(output_sample_rate: u32) -> Self {
        let native_rate = CLOCK / 16.0;
        Scc {
            channels: [
                Channel::new(),
                Channel::new(),
                Channel::new(),
                Channel::new(),
                Channel::new(),
            ],
            waveform_idx: 0,
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
            out += ch.output();
        }
        out * 0.2
    }

    fn get_waveform(&self) -> [i8; 32] {
        match self.waveform_idx {
            0 => WAVE_SINE,
            1 => WAVE_SQUARE,
            2 => WAVE_SAW,
            _ => WAVE_ORGAN,
        }
    }
}

fn midi_to_period(note: u8, detune_cents: f32) -> u16 {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    let period = (CLOCK / (16.0 * 32.0 * freq) - 1.0).round() as i32;
    period.clamp(0, 4095) as u16
}

impl SoundChip for Scc {
    fn chip_id(&self) -> ChipId {
        ChipId::Scc
    }
    fn num_voices(&self) -> usize {
        5
    }
    fn param_info(&self) -> Vec<ParamInfo> {
        scc_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        if param_id == PARAM_WAVEFORM {
            self.waveform_idx = (value as u8).min(3);
            let wave = self.get_waveform();
            for ch in &mut self.channels {
                ch.waveform = wave;
            }
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        if param_id == PARAM_WAVEFORM {
            self.waveform_idx as f32
        } else {
            0.0
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 5 {
            return;
        }
        self.channels[voice].period = midi_to_period(note, detune_cents);
        self.channels[voice].volume = ((velocity as u16 * 15) / 127) as u8;
        self.channels[voice].waveform = self.get_waveform();
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 5 {
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
        let mut chip = Scc::new(44100);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0));
    }

    #[test]
    fn produces_sound() {
        let mut chip = Scc::new(44100);
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01);
    }
}
