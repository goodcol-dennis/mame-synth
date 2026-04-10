#[repr(C)]
pub struct YmfmOpl2Chip {
    _private: [u8; 0],
}

#[repr(C)]
pub struct YmfmOpl3Chip {
    _private: [u8; 0],
}

extern "C" {
    // OPL2
    pub fn ymfm_opl2_create(clock_rate: u32) -> *mut YmfmOpl2Chip;
    pub fn ymfm_opl2_destroy(chip: *mut YmfmOpl2Chip);
    pub fn ymfm_opl2_reset(chip: *mut YmfmOpl2Chip);
    pub fn ymfm_opl2_write(chip: *mut YmfmOpl2Chip, addr: u8, data: u8);
    pub fn ymfm_opl2_generate(chip: *mut YmfmOpl2Chip, left: *mut i32, right: *mut i32);
    pub fn ymfm_opl2_sample_rate(clock: u32) -> u32;

    // OPL3
    pub fn ymfm_opl3_create(clock_rate: u32) -> *mut YmfmOpl3Chip;
    pub fn ymfm_opl3_destroy(chip: *mut YmfmOpl3Chip);
    pub fn ymfm_opl3_reset(chip: *mut YmfmOpl3Chip);
    pub fn ymfm_opl3_write(chip: *mut YmfmOpl3Chip, port: u8, addr: u8, data: u8);
    pub fn ymfm_opl3_generate(
        chip: *mut YmfmOpl3Chip,
        out0: *mut i32,
        out1: *mut i32,
        out2: *mut i32,
        out3: *mut i32,
    );
    pub fn ymfm_opl3_sample_rate(clock: u32) -> u32;
}
