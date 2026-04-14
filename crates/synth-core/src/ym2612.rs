use crate::chip::*;
use crate::ym2612_ffi;

const YM2612_CLOCK: u32 = 7_670_453; // Genesis NTSC: 53.693175 MHz / 7

// Parameter ID scheme:
// 0-9: global params
// 100+op*100+offset: per-operator params (op 0-3)
const PARAM_ALGORITHM: u32 = 0;
const PARAM_FEEDBACK: u32 = 1;
const PARAM_LFO_ENABLE: u32 = 2;
const PARAM_LFO_RATE: u32 = 3;

const fn op_param(op: u32, offset: u32) -> u32 {
    100 + op * 100 + offset
}

const OP_TL: u32 = 0;
const OP_AR: u32 = 1;
const OP_D1R: u32 = 2;
const OP_D2R: u32 = 3;
const OP_SL: u32 = 4;
const OP_RR: u32 = 5;
const OP_MUL: u32 = 6;
const OP_DT: u32 = 7;

pub fn ym2612_param_info() -> Vec<ParamInfo> {
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
            id: PARAM_LFO_ENABLE,
            name: "LFO".into(),
            group: "Global".into(),
            kind: ParamKind::Toggle { default: false },
        },
        ParamInfo {
            id: PARAM_LFO_RATE,
            name: "LFO Rate".into(),
            group: "Global".into(),
            kind: ParamKind::Discrete {
                min: 0,
                max: 7,
                default: 0,
                labels: None,
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
                id: op_param(op, OP_DT),
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
pub struct Ym2612 {
    chip: *mut ym2612_ffi::YmfmChip,
    native_sample_rate: u32,
    output_sample_rate: u32,
    phase_accumulator: f64,
    phase_increment: f64,
    // Shadow state for get_param
    algorithm: u8,
    feedback: u8,
    lfo_enable: bool,
    lfo_rate: u8,
    op_tl: [u8; 4],
    op_ar: [u8; 4],
    op_d1r: [u8; 4],
    op_d2r: [u8; 4],
    op_sl: [u8; 4],
    op_rr: [u8; 4],
    op_mul: [u8; 4],
    op_dt: [u8; 4],
    // Voice allocation
    active_notes: [Option<u8>; 6],
    // Last generated sample (for resampling)
    last_sample: StereoSample,
}

// Safety: only accessed from the audio thread
unsafe impl Send for Ym2612 {}

impl Ym2612 {
    pub fn new(output_sample_rate: u32) -> Self {
        let chip = unsafe { ym2612_ffi::ymfm_ym2612_create(YM2612_CLOCK) };
        let native_rate = unsafe { ym2612_ffi::ymfm_ym2612_sample_rate(YM2612_CLOCK) };

        let mut ym = Ym2612 {
            chip,
            native_sample_rate: native_rate,
            output_sample_rate,
            phase_accumulator: 0.0,
            phase_increment: native_rate as f64 / output_sample_rate as f64,
            algorithm: 0,
            feedback: 0,
            lfo_enable: false,
            lfo_rate: 0,
            op_tl: [40, 40, 40, 0],
            op_ar: [31; 4],
            op_d1r: [0; 4],
            op_d2r: [0; 4],
            op_sl: [0; 4],
            op_rr: [7; 4],
            op_mul: [1; 4],
            op_dt: [0; 4],
            active_notes: [None; 6],
            last_sample: StereoSample::default(),
        };
        ym.init_default_patch();
        ym
    }

    fn write_reg(&self, port: u8, addr: u8, data: u8) {
        unsafe {
            ym2612_ffi::ymfm_ym2612_write(self.chip, port, addr, data);
        }
    }

    fn init_default_patch(&mut self) {
        // Set up a basic FM patch on channel 0
        // Algorithm and feedback
        self.write_reg(0, 0xB0, (self.feedback << 3) | self.algorithm);

        // Enable L+R output
        self.write_reg(0, 0xB4, 0xC0);

        // Set up all 4 operators on channel 0
        // Operator register order in YM2612: op1=0, op2=2, op3=1, op4=3 (weird order)
        let op_offsets: [u8; 4] = [0, 8, 4, 12]; // register offsets for op 1,2,3,4

        for (op, &offset) in op_offsets.iter().enumerate() {
            // DT1/MUL
            self.write_reg(0, 0x30 + offset, (self.op_dt[op] << 4) | self.op_mul[op]);
            // Total Level
            self.write_reg(0, 0x40 + offset, self.op_tl[op]);
            // RS/AR
            self.write_reg(0, 0x50 + offset, self.op_ar[op]);
            // AM/D1R
            self.write_reg(0, 0x60 + offset, self.op_d1r[op]);
            // D2R
            self.write_reg(0, 0x70 + offset, self.op_d2r[op]);
            // SL/RR
            self.write_reg(0, 0x80 + offset, (self.op_sl[op] << 4) | self.op_rr[op]);
        }

        // Key-off all 6 channels so no operators are active at init.
        // Channels 0-2 use values 0x00-0x02; channels 3-5 use 0x04-0x06.
        for ch_val in [0x00u8, 0x01, 0x02, 0x04, 0x05, 0x06] {
            self.write_reg(0, 0x28, ch_val);
        }
    }

    fn generate_one_sample(&mut self) -> StereoSample {
        let mut left: i32 = 0;
        let mut right: i32 = 0;
        unsafe {
            ym2612_ffi::ymfm_ym2612_generate(self.chip, &mut left, &mut right);
        }
        // YMFM outputs ~14-bit signed values; normalize to [-1, 1]
        StereoSample {
            left: left as f32 / 8192.0,
            right: right as f32 / 8192.0,
        }
    }

    fn apply_operator_param(&self, op: usize, param_offset: u32) {
        // Apply to channel 0 only for now (all channels share the same patch)
        let op_offsets: [u8; 4] = [0, 8, 4, 12];
        let offset = op_offsets[op];

        match param_offset {
            OP_TL => self.write_reg(0, 0x40 + offset, self.op_tl[op]),
            OP_AR => self.write_reg(0, 0x50 + offset, self.op_ar[op]),
            OP_D1R => self.write_reg(0, 0x60 + offset, self.op_d1r[op]),
            OP_D2R => self.write_reg(0, 0x70 + offset, self.op_d2r[op]),
            OP_SL | OP_RR => {
                self.write_reg(0, 0x80 + offset, (self.op_sl[op] << 4) | self.op_rr[op])
            }
            OP_MUL | OP_DT => {
                self.write_reg(0, 0x30 + offset, (self.op_dt[op] << 4) | self.op_mul[op])
            }
            _ => {}
        }
    }
}

impl Drop for Ym2612 {
    fn drop(&mut self) {
        unsafe {
            ym2612_ffi::ymfm_ym2612_destroy(self.chip);
        }
    }
}

fn freq_hz_to_fnum_block(freq_hz: f64) -> (u16, u8) {
    // Try different block values to find one where fnum fits in 11 bits (0-2047)
    for block in 0u8..8 {
        let fnum = ((freq_hz * 144.0 * (1u64 << 21) as f64)
            / (YM2612_CLOCK as f64 * (1u64 << block) as f64))
            .round() as u32;
        if fnum <= 2047 {
            return (fnum as u16, block);
        }
    }
    (2047, 7) // clamp to max
}

impl SoundChip for Ym2612 {
    fn chip_id(&self) -> ChipId {
        ChipId::Ym2612
    }

    fn param_info(&self) -> Vec<ParamInfo> {
        ym2612_param_info()
    }

    fn set_param(&mut self, param_id: u32, value: f32) {
        match param_id {
            PARAM_ALGORITHM => {
                self.algorithm = (value as u8).min(7);
                self.write_reg(0, 0xB0, (self.feedback << 3) | self.algorithm);
            }
            PARAM_FEEDBACK => {
                self.feedback = (value as u8).min(7);
                self.write_reg(0, 0xB0, (self.feedback << 3) | self.algorithm);
            }
            PARAM_LFO_ENABLE => {
                self.lfo_enable = value >= 0.5;
                self.write_reg(
                    0,
                    0x22,
                    if self.lfo_enable {
                        0x08 | self.lfo_rate
                    } else {
                        0
                    },
                );
            }
            PARAM_LFO_RATE => {
                self.lfo_rate = (value as u8).min(7);
                if self.lfo_enable {
                    self.write_reg(0, 0x22, 0x08 | self.lfo_rate);
                }
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
                        OP_SL => self.op_sl[op] = v.min(15),
                        OP_RR => self.op_rr[op] = v.min(15),
                        OP_MUL => self.op_mul[op] = v.min(15),
                        OP_DT => self.op_dt[op] = v.min(7),
                        _ => return,
                    }
                    self.apply_operator_param(op, offset);
                }
            }
            _ => {}
        }
    }

    fn get_param(&self, param_id: u32) -> f32 {
        match param_id {
            PARAM_ALGORITHM => self.algorithm as f32,
            PARAM_FEEDBACK => self.feedback as f32,
            PARAM_LFO_ENABLE => {
                if self.lfo_enable {
                    1.0
                } else {
                    0.0
                }
            }
            PARAM_LFO_RATE => self.lfo_rate as f32,
            id if (100..500).contains(&id) => {
                let op = ((id - 100) / 100) as usize;
                let offset = (id - 100) % 100;
                if op < 4 {
                    match offset {
                        OP_TL => self.op_tl[op] as f32,
                        OP_AR => self.op_ar[op] as f32,
                        OP_D1R => self.op_d1r[op] as f32,
                        OP_D2R => self.op_d2r[op] as f32,
                        OP_SL => self.op_sl[op] as f32,
                        OP_RR => self.op_rr[op] as f32,
                        OP_MUL => self.op_mul[op] as f32,
                        OP_DT => self.op_dt[op] as f32,
                        _ => 0.0,
                    }
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    fn num_voices(&self) -> usize {
        6
    }

    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32) {
        if voice >= 6 {
            return;
        }
        let ch = voice;
        let freq_hz =
            440.0 * 2.0f64.powf((note as f64 - 69.0 + detune_cents as f64 / 100.0) / 12.0);
        let (fnum, block) = freq_hz_to_fnum_block(freq_hz);

        let (port, base_ch) = if ch < 3 {
            (0u8, ch as u8)
        } else {
            (1u8, (ch - 3) as u8)
        };

        // Write frequency
        self.write_reg(port, 0xA4 + base_ch, (block << 3) | (fnum >> 8) as u8);
        self.write_reg(port, 0xA0 + base_ch, (fnum & 0xFF) as u8);

        // Apply patch to this channel
        let op_offsets: [u8; 4] = [0, 8, 4, 12];
        for (op, &op_off) in op_offsets.iter().enumerate() {
            let reg_offset = base_ch + op_off;
            self.write_reg(
                port,
                0x30 + reg_offset,
                (self.op_dt[op] << 4) | self.op_mul[op],
            );
            self.write_reg(port, 0x40 + reg_offset, self.op_tl[op]);
            self.write_reg(port, 0x50 + reg_offset, self.op_ar[op]);
            self.write_reg(port, 0x60 + reg_offset, self.op_d1r[op]);
            self.write_reg(port, 0x70 + reg_offset, self.op_d2r[op]);
            self.write_reg(
                port,
                0x80 + reg_offset,
                (self.op_sl[op] << 4) | self.op_rr[op],
            );
        }
        self.write_reg(port, 0xB0 + base_ch, (self.feedback << 3) | self.algorithm);
        self.write_reg(port, 0xB4 + base_ch, 0xC0);

        // Scale carrier TL by velocity
        let velocity_tl = 127 - (velocity as u16 * 127 / 127) as u8;
        let carrier_offset = base_ch + op_offsets[3];
        self.write_reg(port, 0x40 + carrier_offset, velocity_tl.min(self.op_tl[3]));

        // Key on: all 4 operators
        self.write_reg(0, 0x28, 0xF0 | ch as u8);
        self.active_notes[ch] = Some(note);
    }

    fn voice_off(&mut self, voice: usize) {
        if voice >= 6 {
            return;
        }
        self.write_reg(0, 0x28, voice as u8);
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
            ym2612_ffi::ymfm_ym2612_reset(self.chip);
        }
        self.active_notes = [None; 6];
        self.phase_accumulator = 0.0;
        self.init_default_patch();
        // Ensure all channels are silent after reset.
        for ch_val in [0x00u8, 0x01, 0x02, 0x04, 0x05, 0x06] {
            self.write_reg(0, 0x28, ch_val);
        }
    }
}
