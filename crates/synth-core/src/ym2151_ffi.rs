#[repr(C)]
pub struct YmfmOpmChip {
    _private: [u8; 0],
}

extern "C" {
    pub fn ymfm_opm_create(clock_rate: u32) -> *mut YmfmOpmChip;
    pub fn ymfm_opm_destroy(chip: *mut YmfmOpmChip);
    pub fn ymfm_opm_reset(chip: *mut YmfmOpmChip);
    pub fn ymfm_opm_write(chip: *mut YmfmOpmChip, addr: u8, data: u8);
    pub fn ymfm_opm_generate(chip: *mut YmfmOpmChip, left: *mut i32, right: *mut i32);
    pub fn ymfm_opm_sample_rate(clock: u32) -> u32;
}
