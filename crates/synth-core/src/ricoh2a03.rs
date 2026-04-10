use crate::chip::*;

// Ricoh 2A03 — NES/Famicom APU
// 2 pulse channels + 1 triangle + 1 noise + 1 DPCM (DPCM not implemented)

const CPU_CLOCK: f64 = 1_789_773.0; // NTSC NES CPU clock

const PARAM_PULSE1_DUTY: u32 = 0;
const PARAM_PULSE2_DUTY: u32 = 1;
const PARAM_NOISE_MODE: u32 = 2;

// Duty cycle waveforms for pulse channels
const DUTY_CYCLES: [[bool; 8]; 4] = [
    [false, true, false, false, false, false, false, false],  // 12.5%
    [false, true, true, false, false, false, false, false],   // 25%
    [false, true, true, true, true, false, false, false],     // 50%
    [true, false, false, true, true, true, true, true],       // 75% (inverted 25%)
];

pub fn ricoh2a03_param_info() -> Vec<ParamInfo> {
    vec![
        ParamInfo {
            id: PARAM_PULSE1_DUTY,
            name: "Pulse 1 Duty".into(),
            group: "Pulse".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 3,
                default: 2,
                labels: Some(vec![
                    "12.5%".into(),
                    "25%".into(),
                    "50%".into(),
                    "75%".into(),
                ]),
            },
        },
        ParamInfo {
            id: PARAM_PULSE2_DUTY,
            name: "Pulse 2 Duty".into(),
            group: "Pulse".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 3,
                default: 2,
                labels: Some(vec![
                    "12.5%".into(),
                    "25%".into(),
                    "50%".into(),
                    "75%".into(),
                ]),
            },
        },
        ParamInfo {
            id: PARAM_NOISE_MODE,
            name: "Noise Mode".into(),
            group: "Noise".into(),
            kind: ParamKind::Toggle { default: false },
        },
    ]
}

#[derive(Debug, Clone)]
struct PulseChannel {
    period: u16,     // 11-bit timer period
    counter: u16,
    duty: u8,        // 0-3
    step: u8,        // position in 8-step duty sequence
    volume: u8,      // 4-bit
    enabled: bool,
}

impl PulseChannel {
    fn new() -> Self {
        PulseChannel {
            period: 0,
            counter: 0,
            duty: 2,
            step: 0,
            volume: 0,
            enabled: false,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.period;
            self.step = (self.step + 1) % 8;
        }
    }

    fn output(&self) -> f32 {
        if !self.enabled || self.period < 8 {
            return 0.0;
        }
        let on = DUTY_CYCLES[self.duty as usize][self.step as usize];
        if on {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone)]
struct TriangleChannel {
    period: u16,
    counter: u16,
    step: u8,        // 0-31 triangle sequence position
    enabled: bool,
}

// Triangle waveform lookup (32 steps)
const TRIANGLE_SEQ: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

impl TriangleChannel {
    fn new() -> Self {
        TriangleChannel {
            period: 0,
            counter: 0,
            step: 0,
            enabled: false,
        }
    }

    fn tick(&mut self) {
        if !self.enabled {
            return;
        }
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.period;
            self.step = (self.step + 1) % 32;
        }
    }

    fn output(&self) -> f32 {
        if !self.enabled || self.period < 2 {
            return 0.0;
        }
        TRIANGLE_SEQ[self.step as usize] as f32 / 15.0
    }
}

#[derive(Debug, Clone)]
struct NoiseChannel {
    period: u16,
    counter: u16,
    lfsr: u16,
    short_mode: bool, // bit 6 feedback (short) vs bit 1 (long)
    volume: u8,
    enabled: bool,
}

const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

impl NoiseChannel {
    fn new() -> Self {
        NoiseChannel {
            period: 4,
            counter: 0,
            lfsr: 1,
            short_mode: false,
            volume: 0,
            enabled: false,
        }
    }

    fn tick(&mut self) {
        if self.counter > 0 {
            self.counter -= 1;
        } else {
            self.counter = self.period;
            let feedback_bit = if self.short_mode { 6 } else { 1 };
            let feedback = (self.lfsr & 1) ^ ((self.lfsr >> feedback_bit) & 1);
            self.lfsr = (self.lfsr >> 1) | (feedback << 14);
        }
    }

    fn output(&self) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        if (self.lfsr & 1) == 0 {
            self.volume as f32 / 15.0
        } else {
            0.0
        }
    }
}

pub struct Ricoh2a03 {
    pulse1: PulseChannel,
    pulse2: PulseChannel,
    triangle: TriangleChannel,
    noise: NoiseChannel,
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
}

impl Ricoh2a03 {
    pub fn new(output_sample_rate: u32) -> Self {
        Ricoh2a03 {
            pulse1: PulseChannel::new(),
            pulse2: PulseChannel::new(),
            triangle: TriangleChannel::new(),
            noise: NoiseChannel::new(),
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: CPU_CLOCK / output_sample_rate as f64,
        }
    }

    fn tick(&mut self) {
        // Pulse channels clock at CPU/2
        self.pulse1.tick();
        self.pulse2.tick();
        // Triangle clocks at CPU rate
        self.triangle.tick();
        // Noise clocks at CPU rate
        self.noise.tick();
    }

    fn mix_sample(&self) -> f32 {
        // NES mixing approximation
        let pulse_out = 0.00752 * (self.pulse1.output() + self.pulse2.output()) * 15.0;
        let tnd_out = 0.00851 * self.triangle.output() * 15.0
            + 0.00494 * self.noise.output() * 15.0;
        (pulse_out + tnd_out) * 2.0 // boost
    }
}

fn midi_note_to_nes_period(note: u8, detune_cents: f32) -> u16 {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    let period = (CPU_CLOCK / (16.0 * freq) - 1.0).round() as i32;
    period.clamp(0, 2047) as u16
}

fn midi_note_to_tri_period(note: u8, detune_cents: f32) -> u16 {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    let period = (CPU_CLOCK / (32.0 * freq) - 1.0).round() as i32;
    period.clamp(0, 2047) as u16
}

impl SoundChip for Ricoh2a03 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ricoh2a03
    }

    fn num_voices(&self) -> usize {
        4 // pulse1, pulse2, triangle, noise
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        ricoh2a03_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_PULSE1_DUTY => self.pulse1.duty = (value as u8).min(3),
            PARAM_PULSE2_DUTY => self.pulse2.duty = (value as u8).min(3),
            PARAM_NOISE_MODE => self.noise.short_mode = value >= 0.5,
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_PULSE1_DUTY => self.pulse1.duty as f32,
            PARAM_PULSE2_DUTY => self.pulse2.duty as f32,
            PARAM_NOISE_MODE => {
                if self.noise.short_mode {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        let vol = ((velocity as u16 * 15) / 127) as u8;
        match voice {
            0 => {
                self.pulse1.period = midi_note_to_nes_period(note, detune_cents);
                self.pulse1.volume = vol;
                self.pulse1.enabled = true;
            }
            1 => {
                self.pulse2.period = midi_note_to_nes_period(note, detune_cents);
                self.pulse2.volume = vol;
                self.pulse2.enabled = true;
            }
            2 => {
                self.triangle.period = midi_note_to_tri_period(note, detune_cents);
                self.triangle.enabled = true;
            }
            3 => {
                self.noise.volume = vol;
                self.noise.enabled = true;
                // Map note to noise period (lower note = longer period)
                let idx = ((108u8.saturating_sub(note)) as usize * 16 / 88).min(15);
                self.noise.period = NOISE_PERIODS[idx];
            }
            _ => {}
        }
    }

    fn voice_off(&mut self, voice: usize) {
        match voice {
            0 => self.pulse1.enabled = false,
            1 => self.pulse2.enabled = false,
            2 => self.triangle.enabled = false,
            3 => self.noise.enabled = false,
            _ => {}
        }
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
        self.pulse1 = PulseChannel::new();
        self.pulse2 = PulseChannel::new();
        self.triangle = TriangleChannel::new();
        self.noise = NoiseChannel::new();
        self.phase_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_after_creation() {
        let mut chip = Ricoh2a03::new(44100);
        let mut buf = vec![StereoSample::default(); 256];
        chip.generate_samples(&mut buf);
        assert!(buf.iter().all(|s| s.left == 0.0));
    }

    #[test]
    fn pulse_produces_sound() {
        let mut chip = Ricoh2a03::new(44100);
        chip.voice_on(0, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.001);
    }

    #[test]
    fn triangle_produces_sound() {
        let mut chip = Ricoh2a03::new(44100);
        chip.voice_on(2, 60, 127, 0.0);
        let mut buf = vec![StereoSample::default(); 1024];
        chip.generate_samples(&mut buf);
        let peak = buf.iter().map(|s| s.left.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.001);
    }
}
