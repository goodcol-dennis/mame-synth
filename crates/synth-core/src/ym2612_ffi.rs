#[repr(C)]
pub struct YmfmChip {
    _private: [u8; 0],
}

extern "C" {
    pub fn ymfm_ym2612_create(clock_rate: u32) -> *mut YmfmChip;
    pub fn ymfm_ym2612_destroy(chip: *mut YmfmChip);
    pub fn ymfm_ym2612_reset(chip: *mut YmfmChip);
    pub fn ymfm_ym2612_write(chip: *mut YmfmChip, port: u8, addr: u8, data: u8);
    pub fn ymfm_ym2612_generate(chip: *mut YmfmChip, left: *mut i32, right: *mut i32);
    pub fn ymfm_ym2612_sample_rate(clock: u32) -> u32;
}
