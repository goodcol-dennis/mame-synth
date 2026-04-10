use crate::chip::*;
use crate::opl_ffi;

// YMF262 (OPL3) — Sound Blaster Pro 2 / AWE32
// 18 channels (2-op) or 6 channels (4-op) + 12 channels (2-op)
// Stereo output

const OPL3_CLOCK: u32 = 14_318_180;

const PARAM_ALGORITHM: u32 = 0;
const PARAM_FEEDBACK: u32 = 1;
const PARAM_FOUROP: u32 = 2;

const fn op_param(op: u32, offset: u32) -> u32 {
    100 + op * 100 + offset
}

const OP_TL: u32 = 0;
const OP_AR: u32 = 1;
const OP_DR: u32 = 2;
const OP_SL: u32 = 3;
const OP_RR: u32 = 4;
const OP_MUL: u32 = 5;

pub fn ymf262_param_info() -> Vec<ParamInfo> {
    let mut params = vec![
        ParamInfo {
            id: PARAM_ALGORITHM,
            name: "Connection".into(),
            group: "Global".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 1,
                default: 0,
                labels: Some(vec!["FM".into(), "Additive".into()]),
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
            id: PARAM_FOUROP,
            name: "4-Op Mode".into(),
            group: "Global".into(),
            kind: ParamKind::Toggle { default: false },
        },
    ];

    for op in 0..2u32 {
        let group = format!("Operator {}", op + 1);
        params.extend_from_slice(&[
            ParamInfo {
                id: op_param(op, OP_TL),
                name: "Level".into(),
                group: group.clone(),
                kind: ParamKind::Continuous {
                    min: 0.0,
                    max: 63.0,
                    default: if op == 1 { 0.0 } else { 20.0 },
                },
            },
            ParamInfo {
                id: op_param(op, OP_AR),
                name: "Attack".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
                    default: 15,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_DR),
                name: "Decay".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
                    default: 0,
                    labels: None,
                },
            },
            ParamInfo {
                id: op_param(op, OP_SL),
                name: "Sustain".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 15,
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
        ]);
    }
    params
}

#[allow(dead_code)]
pub struct Ymf262 {
    chip: *mut opl_ffi::YmfmOpl3Chip,
    native_sample_rate: u32,
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
    algorithm: u8,
    feedback: u8,
    op_tl: [u8; 2],
    op_ar: [u8; 2],
    op_dr: [u8; 2],
    op_sl: [u8; 2],
    op_rr: [u8; 2],
    op_mul: [u8; 2],
    active_notes: [Option<u8>; 18],
    last_sample: StereoSample,
}

unsafe impl Send for Ymf262 {}

impl Ymf262 {
    pub fn new(output_sample_rate: u32) -> Self {
        let chip = unsafe { opl_ffi::ymfm_opl3_create(OPL3_CLOCK) };
        let native_rate = unsafe { opl_ffi::ymfm_opl3_sample_rate(OPL3_CLOCK) };
        let ym = Ymf262 {
            chip,
            native_sample_rate: native_rate,
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: native_rate as f64 / output_sample_rate as f64,
            algorithm: 0,
            feedback: 0,
            op_tl: [20, 0],
            op_ar: [15; 2],
            op_dr: [0; 2],
            op_sl: [0; 2],
            op_rr: [7; 2],
            op_mul: [1; 2],
            active_notes: [None; 18],
            last_sample: StereoSample::default(),
        };
        // Enable OPL3 mode
        ym.write_reg(1, 0x05, 0x01); // NEW bit
        ym.init_patch(0);
        ym
    }

    fn write_reg(&self, port: u8, addr: u8, data: u8) {
        unsafe {
            opl_ffi::ymfm_opl3_write(self.chip, port, addr, data);
        }
    }

    fn init_patch(&self, ch: u8) {
        let port: u8 = if ch < 9 { 0 } else { 1 };
        let local_ch = if ch < 9 { ch } else { ch - 9 };
        let op_offsets = opl2_op_offsets(local_ch);
        for (op, &off) in op_offsets.iter().enumerate() {
            self.write_reg(port, 0x20 + off, self.op_mul[op] & 0x0F);
            self.write_reg(port, 0x40 + off, self.op_tl[op] & 0x3F);
            self.write_reg(
                port,
                0x60 + off,
                (self.op_ar[op] << 4) | (self.op_dr[op] & 0x0F),
            );
            self.write_reg(
                port,
                0x80 + off,
                (self.op_sl[op] << 4) | (self.op_rr[op] & 0x0F),
            );
        }
        // FB/CNT + stereo (L+R)
        self.write_reg(
            port,
            0xC0 + local_ch,
            0x30 | (self.feedback << 1) | self.algorithm,
        );
    }
}

fn opl2_op_offsets(ch: u8) -> [u8; 2] {
    let base = match ch {
        0..=2 => ch,
        3..=5 => ch + 5,
        6..=8 => ch + 10,
        _ => 0,
    };
    [base, base + 3]
}

fn midi_note_to_opl3_fnum(note: u8, detune_cents: f32) -> (u16, u8) {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    for block in 0u8..8 {
        let fnum =
            (freq * (1u64 << (20 - block)) as f64 / (OPL3_CLOCK as f64 / 288.0)).round() as u32;
        if fnum <= 1023 {
            return (fnum as u16, block);
        }
    }
    (1023, 7)
}

impl Drop for Ymf262 {
    fn drop(&mut self) {
        unsafe {
            opl_ffi::ymfm_opl3_destroy(self.chip);
        }
    }
}

impl SoundChip for Ymf262 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ymf262
    }
    fn num_voices(&self) -> usize {
        18
    }
    fn param_info(&self) -> Vec<ParamInfo> {
        ymf262_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_ALGORITHM => {
                self.algorithm = (value as u8).min(1);
                self.init_patch(0);
            }
            PARAM_FEEDBACK => {
                self.feedback = (value as u8).min(7);
                self.init_patch(0);
            }
            PARAM_FOUROP => {} // TODO: 4-op mode switching
            id if (100..300).contains(&id) => {
                let op = ((id - 100) / 100) as usize;
                let offset = (id - 100) % 100;
                if op < 2 {
                    let v = value as u8;
                    match offset {
                        OP_TL => self.op_tl[op] = v.min(63),
                        OP_AR => self.op_ar[op] = v.min(15),
                        OP_DR => self.op_dr[op] = v.min(15),
                        OP_SL => self.op_sl[op] = v.min(15),
                        OP_RR => self.op_rr[op] = v.min(15),
                        OP_MUL => self.op_mul[op] = v.min(15),
                        _ => return,
                    }
                    self.init_patch(0);
                }
            }
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_ALGORITHM => self.algorithm as f32,
            PARAM_FEEDBACK => self.feedback as f32,
            PARAM_FOUROP => 0.0,
            id if (100..300).contains(&id) => {
                let op = ((id - 100) / 100) as usize;
                let offset = (id - 100) % 100;
                if op < 2 {
                    match offset {
                        OP_TL => self.op_tl[op] as f32,
                        OP_AR => self.op_ar[op] as f32,
                        OP_DR => self.op_dr[op] as f32,
                        OP_SL => self.op_sl[op] as f32,
                        OP_RR => self.op_rr[op] as f32,
                        OP_MUL => self.op_mul[op] as f32,
                        _ => 0.0,
                    }
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    fn voice_on(&mut self, voice: usize, note: u8, _velocity: u8, detune_cents: f32) {
        if voice >= 18 {
            return;
        }
        let ch = voice as u8;
        let port: u8 = if ch < 9 { 0 } else { 1 };
        let local_ch = if ch < 9 { ch } else { ch - 9 };
        self.init_patch(ch);
        let (fnum, block) = midi_note_to_opl3_fnum(note, detune_cents);
        self.write_reg(port, 0xA0 + local_ch, (fnum & 0xFF) as u8);
        self.write_reg(
            port,
            0xB0 + local_ch,
            0x20 | ((block & 0x07) << 2) | ((fnum >> 8) as u8 & 0x03),
        );
        self.active_notes[voice] = Some(note);
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 18 {
            return;
        }
        let ch = voice as u8;
        let port: u8 = if ch < 9 { 0 } else { 1 };
        let local_ch = if ch < 9 { ch } else { ch - 9 };
        self.write_reg(port, 0xB0 + local_ch, 0x00);
        self.active_notes[voice] = None;
    }

    fn generate_samples(&mut self, output: &mut [StereoSample]) {
        for sample in output.iter_mut() {
            self.phase_accumulator += self.phase_increment;
            while self.phase_accumulator >= 1.0 {
                let (mut o0, mut o1, mut o2, mut o3) = (0i32, 0i32, 0i32, 0i32);
                unsafe {
                    opl_ffi::ymfm_opl3_generate(self.chip, &mut o0, &mut o1, &mut o2, &mut o3);
                }
                // OPL3 has 4 outputs — mix to stereo
                self.last_sample = StereoSample {
                    left: (o0 + o2) as f32 / 16384.0,
                    right: (o1 + o3) as f32 / 16384.0,
                };
                self.phase_accumulator -= 1.0;
            }
            *sample = self.last_sample;
        }
    }

    fn reset(&mut self) {
        unsafe {
            opl_ffi::ymfm_opl3_reset(self.chip);
        }
        self.active_notes = [None; 18];
        self.phase_accumulator = 0.0;
        self.write_reg(1, 0x05, 0x01); // Re-enable OPL3 mode
        self.init_patch(0);
    }
}
