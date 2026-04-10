#include "ymfm_opl_wrapper.h"
#include "../ymfm/src/ymfm_opl.h"

class OplInterface : public ymfm::ymfm_interface {
public:
    void ymfm_sync_mode_write(uint8_t) override {}
    void ymfm_sync_check_interrupts() override {}
    void ymfm_set_timer(uint32_t, int32_t) override {}
    void ymfm_set_busy_end(uint32_t) override {}
    bool ymfm_is_busy() override { return false; }
};

// ─── OPL2 (YM3812) ──────────────────────────────────────────────────

struct YmfmOpl2Chip {
    OplInterface interface;
    ymfm::ym3812 chip;
    YmfmOpl2Chip(uint32_t) : interface(), chip(interface) { chip.reset(); }
};

extern "C" {

YmfmOpl2Chip* ymfm_opl2_create(uint32_t clock_rate) {
    return new YmfmOpl2Chip(clock_rate);
}
void ymfm_opl2_destroy(YmfmOpl2Chip* chip) { delete chip; }
void ymfm_opl2_reset(YmfmOpl2Chip* chip) { chip->chip.reset(); }

void ymfm_opl2_write(YmfmOpl2Chip* chip, uint8_t addr, uint8_t data) {
    chip->chip.write_address(addr);
    chip->chip.write_data(data);
}

void ymfm_opl2_generate(YmfmOpl2Chip* chip, int32_t* left, int32_t* right) {
    ymfm::ym3812::output_data output;
    chip->chip.generate(&output);
    *left = output.data[0];
    *right = output.data[0]; // OPL2 is mono — duplicate to stereo
}

uint32_t ymfm_opl2_sample_rate(uint32_t clock) {
    return clock / 72; // OPL2 divides by 72
}

} // extern "C"

// ─── OPL3 (YMF262) ──────────────────────────────────────────────────

struct YmfmOpl3Chip {
    OplInterface interface;
    ymfm::ymf262 chip;
    YmfmOpl3Chip(uint32_t) : interface(), chip(interface) { chip.reset(); }
};

extern "C" {

YmfmOpl3Chip* ymfm_opl3_create(uint32_t clock_rate) {
    return new YmfmOpl3Chip(clock_rate);
}
void ymfm_opl3_destroy(YmfmOpl3Chip* chip) { delete chip; }
void ymfm_opl3_reset(YmfmOpl3Chip* chip) { chip->chip.reset(); }

void ymfm_opl3_write(YmfmOpl3Chip* chip, uint8_t port, uint8_t addr, uint8_t data) {
    uint32_t offset = port * 2;
    chip->chip.write(offset, addr);
    chip->chip.write(offset + 1, data);
}

void ymfm_opl3_generate(YmfmOpl3Chip* chip, int32_t* out0, int32_t* out1, int32_t* out2, int32_t* out3) {
    ymfm::ymf262::output_data output;
    chip->chip.generate(&output);
    *out0 = output.data[0];
    *out1 = output.data[1];
    *out2 = output.data[2];
    *out3 = output.data[3];
}

uint32_t ymfm_opl3_sample_rate(uint32_t clock) {
    return clock / 288; // OPL3 divides by 288
}

} // extern "C"
