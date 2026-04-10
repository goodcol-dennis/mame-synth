use crate::chip::*;
use crate::ym2151_ffi;

// YM2151 (OPM) — 8-channel, 4-operator FM synthesizer
// Used in arcade machines (Capcom CPS1, Sega System 16), Sharp X68000

const YM2151_CLOCK: u32 = 3_579_545; // Common arcade clock

const PARAM_ALGORITHM: u32 = 0;
const PARAM_FEEDBACK: u32 = 1;
const PARAM_LFO_FREQ: u32 = 2;
const PARAM_LFO_WAVE: u32 = 3;

const fn op_param(op: u32, offset: u32) -> u32 {
    100 + op * 100 + offset
}

const OP_TL: u32 = 0;
const OP_AR: u32 = 1;
const OP_D1R: u32 = 2;
const OP_D2R: u32 = 3;
const OP_RR: u32 = 4;
const OP_SL: u32 = 5;
const OP_MUL: u32 = 6;
const OP_DT1: u32 = 7;

pub fn ym2151_param_info() -> Vec<ParamInfo> {
    let mut params = vec![
        ParamInfo {
            id: PARAM_ALGORITHM,
            name: "Algorithm".into(),
            group: "Global".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 7,
                default: 0,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_FEEDBACK,
            name: "Feedback".into(),
            group: "Global".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 7,
                default: 0,
                labels: None,
            },
        },
        ParamInfo {
            id: PARAM_LFO_FREQ,
            name: "LFO Speed".into(),
            group: "LFO".into(),
            kind: ParamKind::Continuous {
                min: 0.0,
                max: 255.0,
                default: 0.0,
            },
        },
        ParamInfo {
            id: PARAM_LFO_WAVE,
            name: "LFO Wave".into(),
            group: "LFO".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 3,
                default: 0,
                labels: Some(vec![
                    "Saw".into(),
                    "Square".into(),
                    "Tri".into(),
                    "Noise".into(),
                ]),
            },
        },
    ];

    for op in 0..4u32 {
        let group = format!("Operator {}", op + 1);
        params.extend_from_slice(&[
            ParamInfo {
                id: op_param(op, OP_TL),
                name: "Total Level".into(),
                group: group.clone(),
                kind: ParamKind::Continuous {
                    min: 0.0,
                    max: 127.0,
                    default: if op == 3 { 0.0 } else { 40.0 },
                },
            },
            ParamInfo {
                id: op_param(op, OP_AR),
                name: "Attack".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 31,
                    default: 31,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_D1R),
                name: "Decay 1".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 31,
                    default: 0,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_D2R),
                name: "Decay 2".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 31,
                    default: 0,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_RR),
                name: "Release".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
                    default: 7,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_SL),
                name: "Sustain Level".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
                    default: 0,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_MUL),
                name: "Multiply".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
                    default: 1,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_DT1),
                name: "Detune".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 7,
                    default: 0,
                    labels: None,
                },
            },
        ]);
    }

    params
}

#[allow(dead_code)]
pub struct Ym2151 {
    chip: *mut ym2151_ffi::YmfmOpmChip,
    native_sample_rate: u32,
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
    // Shadow state
    algorithm: u8,
    feedback: u8,
    op_tl: [u8; 4],
    op_ar: [u8; 4],
    op_d1r: [u8; 4],
    op_d2r: [u8; 4],
    op_rr: [u8; 4],
    op_sl: [u8; 4],
    op_mul: [u8; 4],
    op_dt1: [u8; 4],
    active_notes: [Option<u8>; 8],
    last_sample: StereoSample,
}

unsafe impl Send for Ym2151 {}

impl Ym2151 {
    pub fn new(output_sample_rate: u32) -> Self {
        let chip = unsafe { ym2151_ffi::ymfm_opm_create(YM2151_CLOCK) };
        let native_rate = unsafe { ym2151_ffi::ymfm_opm_sample_rate(YM2151_CLOCK) };

        let mut ym = Ym2151 {
            chip,
            native_sample_rate: native_rate,
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: native_rate as f64 / output_sample_rate as f64,
            algorithm: 0,
            feedback: 0,
            op_tl: [40, 40, 40, 0],
            op_ar: [31; 4],
            op_d1r: [0; 4],
            op_d2r: [0; 4],
            op_rr: [7; 4],
            op_sl: [0; 4],
            op_mul: [1; 4],
            op_dt1: [0; 4],
            active_notes: [None; 8],
            last_sample: StereoSample::default(),
        };
        ym.init_default_patch();
        ym
    }

    fn write_reg(&self, addr: u8, data: u8) {
        unsafe {
            ym2151_ffi::ymfm_opm_write(self.chip, addr, data);
        }
    }

    fn init_default_patch(&self) {
        // Set up channel 0 with default patch
        // YM2151 operator order: M1(0), C1(1), M2(2), C2(3)
        // Register layout: base + ch, where operators are at +0, +8, +16, +24
        let op_offsets: [u8; 4] = [0, 16, 8, 24];

        for (op, &offset) in op_offsets.iter().enumerate() {
            // DT1/MUL
            self.write_reg(0x40 + offset, (self.op_dt1[op] << 4) | self.op_mul[op]);
            // TL
            self.write_reg(0x60 + offset, self.op_tl[op]);
            // KS/AR
            self.write_reg(0x80 + offset, self.op_ar[op]);
            // D1R
            self.write_reg(0xA0 + offset, self.op_d1r[op]);
            // DT2/D2R
            self.write_reg(0xC0 + offset, self.op_d2r[op]);
            // SL/RR
            self.write_reg(0xE0 + offset, (self.op_sl[op] << 4) | self.op_rr[op]);
        }

        // RL/FB/Algorithm for channel 0
        self.write_reg(0x20, 0xC0 | (self.feedback << 3) | self.algorithm);
    }

    fn generate_one_sample(&mut self) -> StereoSample {
        let mut left: i32 = 0;
        let mut right: i32 = 0;
        unsafe {
            ym2151_ffi::ymfm_opm_generate(self.chip, &mut left, &mut right);
        }
        StereoSample {
            left: left as f32 / 8192.0,
            right: right as f32 / 8192.0,
        }
    }

    fn apply_patch_to_channel(&self, ch: u8) {
        let op_offsets: [u8; 4] = [0, 16, 8, 24];
        for (op, &op_off) in op_offsets.iter().enumerate() {
            let reg = ch + op_off;
            self.write_reg(0x40 + reg, (self.op_dt1[op] << 4) | self.op_mul[op]);
            self.write_reg(0x60 + reg, self.op_tl[op]);
            self.write_reg(0x80 + reg, self.op_ar[op]);
            self.write_reg(0xA0 + reg, self.op_d1r[op]);
            self.write_reg(0xC0 + reg, self.op_d2r[op]);
            self.write_reg(0xE0 + reg, (self.op_sl[op] << 4) | self.op_rr[op]);
        }
        self.write_reg(0x20 + ch, 0xC0 | (self.feedback << 3) | self.algorithm);
    }
}

impl Drop for Ym2151 {
    fn drop(&mut self) {
        unsafe {
            ym2151_ffi::ymfm_opm_destroy(self.chip);
        }
    }
}

// YM2151 uses a key code (KC) and key fraction (KF) for pitch
fn midi_note_to_kc_kf(note: u8, detune_cents: f32) -> (u8, u8) {
    // KC = octave * 16 + note_within_octave_mapped
    // The YM2151 uses a non-linear note mapping within each octave
    let note_f = note as f32 + detune_cents / 100.0;
    let octave = ((note_f / 12.0).floor() as i32 - 1).clamp(0, 7) as u8;
    let semitone = (note_f % 12.0).max(0.0);
    // YM2151 note codes within octave: 0,1,2,4,5,6,8,9,10,12,13,14
    let note_codes: [u8; 12] = [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14];
    let semi_idx = (semitone as usize).min(11);
    let kc = (octave << 4) | note_codes[semi_idx];
    // Key fraction from the fractional semitone part
    let frac = semitone - semitone.floor();
    let kf = (frac * 63.0) as u8;
    (kc, kf << 2) // KF is in bits 7-2
}

impl SoundChip for Ym2151 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ym2151
    }

    fn num_voices(&self) -> usize {
        8
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        ym2151_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_ALGORITHM => {
                self.algorithm = (value as u8).min(7);
                self.write_reg(0x20, 0xC0 | (self.feedback << 3) | self.algorithm);
            }
            PARAM_FEEDBACK => {
                self.feedback = (value as u8).min(7);
                self.write_reg(0x20, 0xC0 | (self.feedback << 3) | self.algorithm);
            }
            PARAM_LFO_FREQ => {
                self.write_reg(0x18, (value as u8));
            }
            PARAM_LFO_WAVE => {
                self.write_reg(0x1B, (value as u8).min(3));
            }
            id if (100..500).contains(&id) => {
                let op = ((id - 100) / 100) as usize;
                let offset = (id - 100) % 100;
                if op < 4 {
                    let v = value as u8;
                    match offset {
                        OP_TL => self.op_tl[op] = v.min(127),
                        OP_AR => self.op_ar[op] = v.min(31),
                        OP_D1R => self.op_d1r[op] = v.min(31),
                        OP_D2R => self.op_d2r[op] = v.min(31),
                        OP_RR => self.op_rr[op] = v.min(15),
                        OP_SL => self.op_sl[op] = v.min(15),
                        OP_MUL => self.op_mul[op] = v.min(15),
                        OP_DT1 => self.op_dt1[op] = v.min(7),
                        _ => return,
                    }
                    // Apply to channel 0
                    let op_offsets: [u8; 4] = [0, 16, 8, 24];
                    let reg_off = op_offsets[op];
                    match offset {
                        OP_TL => self.write_reg(0x60 + reg_off, self.op_tl[op]),
                        OP_AR => self.write_reg(0x80 + reg_off, self.op_ar[op]),
                        OP_D1R => self.write_reg(0xA0 + reg_off, self.op_d1r[op]),
                        OP_D2R => self.write_reg(0xC0 + reg_off, self.op_d2r[op]),
                        OP_SL | OP_RR => {
                            self.write_reg(
                                0xE0 + reg_off,
                                (self.op_sl[op] << 4) | self.op_rr[op],
                            );
                        }
                        OP_MUL | OP_DT1 => {
                            self.write_reg(
                                0x40 + reg_off,
                                (self.op_dt1[op] << 4) | self.op_mul[op],
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_ALGORITHM => self.algorithm as f32,
            PARAM_FEEDBACK => self.feedback as f32,
            id if (100..500).contains(&id) => {
                let op = ((id - 100) / 100) as usize;
                let offset = (id - 100) % 100;
                if op < 4 {
                    match offset {
                        OP_TL => self.op_tl[op] as f32,
                        OP_AR => self.op_ar[op] as f32,
                        OP_D1R => self.op_d1r[op] as f32,
                        OP_D2R => self.op_d2r[op] as f32,
                        OP_RR => self.op_rr[op] as f32,
                        OP_SL => self.op_sl[op] as f32,
                        OP_MUL => self.op_mul[op] as f32,
                        OP_DT1 => self.op_dt1[op] as f32,
                        _ => 0.0,
                    }
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 8 {
            return;
        }
        let ch = voice as u8;
        let (kc, kf) = midi_note_to_kc_kf(note, detune_cents);

        // Apply patch to this channel
        self.apply_patch_to_channel(ch);

        // Set pitch
        self.write_reg(0x28 + ch, kc); // KC
        self.write_reg(0x30 + ch, kf); // KF

        // Velocity → carrier TL
        let velocity_tl = 127 - (velocity as u16 * 127 / 127) as u8;
        let op_offsets: [u8; 4] = [0, 16, 8, 24];
        self.write_reg(0x60 + ch + op_offsets[3], velocity_tl.min(self.op_tl[3]));

        // Key on: all 4 operators on channel
        self.write_reg(0x08, 0x78 | ch); // SN=0111 (all ops), CH=voice
        self.active_notes[voice] = Some(note);
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 8 {
            return;
        }
        // Key off: all operators
        self.write_reg(0x08, voice as u8); // SN=0000, CH=voice
        self.active_notes[voice] = None;
    }

    fn generate_samples(&mut self, output: &mut [StereoSample]) {
        for sample in output.iter_mut() {
            self.phase_accumulator += self.phase_increment;
            while self.phase_accumulator >= 1.0 {
                self.last_sample = self.generate_one_sample();
                self.phase_accumulator -= 1.0;
            }
            *sample = self.last_sample;
        }
    }

    fn reset(&mut self) {
        unsafe {
            ym2151_ffi::ymfm_opm_reset(self.chip);
        }
        self.active_notes = [None; 8];
        self.phase_accumulator = 0.0;
        self.init_default_patch();
    }
}
