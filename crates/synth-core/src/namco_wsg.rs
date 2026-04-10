use crate::chip::*;

// Namco WSG (Waveform Sound Generator)
// Used in Pac-Man, Galaga, Dig Dug, Rally-X
// 3 channels (Pac-Man) or 8 channels (Galaga), 32-byte 4-bit waveforms
// We implement the 3-channel version (Pac-Man / Rally-X)

const CLOCK: f64 = 3_072_000.0; // Pac-Man master clock / 32

const PARAM_WAVEFORM: u32 = 0;

// Pac-Man waveforms (4-bit samples, 0-15)
const WAVE_BELL: [u8; 32] = [
    7, 9, 11, 13, 14, 15, 14, 13, 11, 9, 7, 5, 3, 1, 0, 0, 0, 0, 1, 3, 5, 7, 9, 11, 13, 14, 15, 14,
    13, 11, 9, 7,
];
const WAVE_SIREN: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4,
    3, 2, 1, 0,
];
const WAVE_SQUARE: [u8; 32] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0,
];
const WAVE_CHOMP: [u8; 32] = [
    7, 10, 12, 14, 15, 14, 12, 10, 7, 4, 2, 0, 0, 2, 4, 7, 10, 13, 15, 13, 10, 7, 4, 1, 0, 1, 4, 7,
    10, 12, 14, 12,
];

pub fn namco_wsg_param_info() -> Vec<ParamInfo> {
    vec![ParamInfo {
        id: PARAM_WAVEFORM,
        name: "Waveform".into(),
        group: "Wavetable".into(),
        kind: ParamKind::Discrete {
            min: 0,
            max: 3,
            default: 0,
            labels: Some(vec![
                "Bell".into(),
                "Siren".into(),
                "Square".into(),
                "Chomp".into(),
            ]),
        },
    }]
}

#[derive(Debug, Clone)]
struct Channel {
    frequency: u32, // 20-bit frequency accumulator increment
    accumulator: u32,
    volume: u8, // 4-bit
    waveform: [u8; 32],
}

impl Channel {
    fn new() -> Self {
        Channel {
            frequency: 0,
            accumulator: 0,
            volume: 0,
            waveform: WAVE_BELL,
        }
    }

    fn tick(&mut self) {
        self.accumulator = self.accumulator.wrapping_add(self.frequency);
    }

    fn output(&self) -> f32 {
        if self.volume == 0 || self.frequency == 0 {
            return 0.0;
        }
        let idx = ((self.accumulator >> 15) & 0x1F) as usize;
        let sample = (self.waveform[idx] as f32 / 7.5) - 1.0; // normalize to [-1, 1]
        sample * (self.volume as f32 / 15.0)
    }
}

pub struct NamcoWsg {
    channels: [Channel; 3],
    waveform_idx: u8,
    #[allow(dead_code)]
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl NamcoWsg {
    pub fn new(output_sample_rate: u32) -> Self {
        let native_rate = CLOCK / 32.0;
        NamcoWsg {
            channels: [Channel::new(), Channel::new(), Channel::new()],
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
        out * 0.33
    }

    fn get_waveform(&self) -> [u8; 32] {
        match self.waveform_idx {
            0 => WAVE_BELL,
            1 => WAVE_SIREN,
            2 => WAVE_SQUARE,
            _ => WAVE_CHOMP,
        }
    }
}

fn midi_to_freq_inc(note: u8, detune_cents: f32) -> u32 {
    let freq_hz = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    // freq_inc = freq_hz * 2^20 / (clock / 32)
    let native_rate = CLOCK / 32.0;
    (freq_hz * 32.0 * (1u64 << 15) as f64 / native_rate).round() as u32
}

impl SoundChip for NamcoWsg {
    fn chip_id(&self) -> ChipId {
        ChipId::NamcoWsg
    }
    fn num_voices(&self) -> usize {
        3
    }
    fn param_info(&self) -> Vec<ParamInfo> {
        namco_wsg_param_info()
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
        if voice >= 3 {
            return;
        }
        self.channels[voice].frequency = midi_to_freq_inc(note, detune_cents);
        self.channels[voice].volume = ((velocity as u16 * 15) / 127) as u8;
        self.channels[voice].waveform = self.get_waveform();
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 3 {
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
        self.channels = [Channel::new(), Channel::new(), Channel::new()];
        self.phase_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_after_creation() {
        let mut chip = NamcoWsg::new(44100);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0));
    }

    #[test]
    fn produces_sound() {
        let mut chip = NamcoWsg::new(44100);
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.01);
    }
}
