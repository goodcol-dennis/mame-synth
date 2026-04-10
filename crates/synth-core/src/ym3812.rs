use crate::chip::*;
use crate::opl_ffi;

// YM3812 (OPL2) — AdLib / Sound Blaster
// 9 channels, 2 operators per channel, FM synthesis

const OPL2_CLOCK: u32 = 3_579_545;

const PARAM_ALGORITHM: u32 = 0; // 0=FM, 1=additive
const PARAM_FEEDBACK: u32 = 1;

const fn op_param(op: u32, offset: u32) -> u32 {
    100 + op * 100 + offset
}

const OP_TL: u32 = 0;
const OP_AR: u32 = 1;
const OP_DR: u32 = 2;
const OP_SL: u32 = 3;
const OP_RR: u32 = 4;
const OP_MUL: u32 = 5;
const OP_KSL: u32 = 6;

pub fn ym3812_param_info() -> Vec<ParamInfo> {
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
            ParamInfo {
                id: op_param(op, OP_KSL),
                name: "Key Scale".into(),
                group: group.clone(),
                kind: ParamKind::Discrete {
                    min: 0,
                    max: 3,
                    default: 0,
                    labels: None,
                },
            },
        ]);
    }
    params
}

#[allow(dead_code)]
pub struct Ym3812 {
    chip: *mut opl_ffi::YmfmOpl2Chip,
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
    op_ksl: [u8; 2],
    active_notes: [Option<u8>; 9],
    last_sample: StereoSample,
}

unsafe impl Send for Ym3812 {}

impl Ym3812 {
    pub fn new(output_sample_rate: u32) -> Self {
        let chip = unsafe { opl_ffi::ymfm_opl2_create(OPL2_CLOCK) };
        let native_rate = unsafe { opl_ffi::ymfm_opl2_sample_rate(OPL2_CLOCK) };
        let ym = Ym3812 {
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
            op_ksl: [0; 2],
            active_notes: [None; 9],
            last_sample: StereoSample::default(),
        };
        ym.init_patch(0);
        ym
    }

    fn write_reg(&self, addr: u8, data: u8) {
        unsafe {
            opl_ffi::ymfm_opl2_write(self.chip, addr, data);
        }
    }

    fn init_patch(&self, ch: u8) {
        // OPL2 operator offsets: op1 at +0, op2 at +3 (for channels 0-2)
        // Register layout is complex — operators are at specific offsets
        let op_offsets = opl2_op_offsets(ch);
        for (op, &off) in op_offsets.iter().enumerate() {
            // 0x20: AM/VIB/EG/KSR/MUL
            self.write_reg(0x20 + off, self.op_mul[op] & 0x0F);
            // 0x40: KSL/TL
            self.write_reg(0x40 + off, (self.op_ksl[op] << 6) | (self.op_tl[op] & 0x3F));
            // 0x60: AR/DR
            self.write_reg(0x60 + off, (self.op_ar[op] << 4) | (self.op_dr[op] & 0x0F));
            // 0x80: SL/RR
            self.write_reg(0x80 + off, (self.op_sl[op] << 4) | (self.op_rr[op] & 0x0F));
        }
        // 0xC0: FB/Connection
        self.write_reg(0xC0 + ch, (self.feedback << 1) | self.algorithm);
    }
}

// OPL2 operator register offset mapping
fn opl2_op_offsets(ch: u8) -> [u8; 2] {
    // Channels 0-2: ops at +0,+3  |  3-5: +8,+11  |  6-8: +16,+19
    let base = match ch {
        0..=2 => ch,
        3..=5 => ch + 5,
        6..=8 => ch + 10,
        _ => 0,
    };
    [base, base + 3]
}

fn midi_note_to_opl_fnum(note: u8, detune_cents: f32) -> (u16, u8) {
    let freq = 440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
    for block in 0u8..8 {
        let fnum =
            (freq * (1u64 << (20 - block)) as f64 / (OPL2_CLOCK as f64 / 72.0)).round() as u32;
        if fnum <= 1023 {
            return (fnum as u16, block);
        }
    }
    (1023, 7)
}

impl Drop for Ym3812 {
    fn drop(&mut self) {
        unsafe {
            opl_ffi::ymfm_opl2_destroy(self.chip);
        }
    }
}

impl SoundChip for Ym3812 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ym3812
    }
    fn num_voices(&self) -> usize {
        9
    }
    fn param_info(&self) -> Vec<ParamInfo> {
        ym3812_param_info()
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
                        OP_KSL => self.op_ksl[op] = v.min(3),
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
                        OP_KSL => self.op_ksl[op] as f32,
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
        if voice >= 9 {
            return;
        }
        let ch = voice as u8;
        self.init_patch(ch);
        let (fnum, block) = midi_note_to_opl_fnum(note, detune_cents);
        // Key off first
        self.write_reg(
            0xB0 + ch,
            ((block & 0x07) << 2) | ((fnum >> 8) as u8 & 0x03),
        );
        // Set frequency
        self.write_reg(0xA0 + ch, (fnum & 0xFF) as u8);
        // Key on
        self.write_reg(
            0xB0 + ch,
            0x20 | ((block & 0x07) << 2) | ((fnum >> 8) as u8 & 0x03),
        );
        self.active_notes[voice] = Some(note);
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 9 {
            return;
        }
        let ch = voice as u8;
        // Key off: clear bit 5 of 0xB0
        let block_fnum = 0x00u8; // just clear key-on bit
        self.write_reg(0xB0 + ch, block_fnum);
        self.active_notes[voice] = None;
    }

    fn generate_samples(&mut self, output: &mut [StereoSample]) {
        for sample in output.iter_mut() {
            self.phase_accumulator += self.phase_increment;
            while self.phase_accumulator >= 1.0 {
                let mut l: i32 = 0;
                let mut r: i32 = 0;
                unsafe {
                    opl_ffi::ymfm_opl2_generate(self.chip, &mut l, &mut r);
                }
                self.last_sample = StereoSample {
                    left: l as f32 / 8192.0,
                    right: r as f32 / 8192.0,
                };
                self.phase_accumulator -= 1.0;
            }
            *sample = self.last_sample;
        }
    }

    fn reset(&mut self) {
        unsafe {
            opl_ffi::ymfm_opl2_reset(self.chip);
        }
        self.active_notes = [None; 9];
        self.phase_accumulator = 0.0;
        self.init_patch(0);
    }
}
