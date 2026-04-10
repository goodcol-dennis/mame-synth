#include "ymfm_opm_wrapper.h"
#include "../ymfm/src/ymfm_opm.h"

class OpmInterface : public ymfm::ymfm_interface {
public:
    void ymfm_sync_mode_write(uint8_t) override {}
    void ymfm_sync_check_interrupts() override {}
    void ymfm_set_timer(uint32_t, int32_t) override {}
    void ymfm_set_busy_end(uint32_t) override {}
    bool ymfm_is_busy() override { return false; }
};

struct YmfmOpmChip {
    OpmInterface interface;
    ymfm::ym2151 chip;

    YmfmOpmChip(uint32_t) : interface(), chip(interface) {
        chip.reset();
    }
};

extern "C" {

YmfmOpmChip* ymfm_opm_create(uint32_t clock_rate) {
    return new YmfmOpmChip(clock_rate);
}

void ymfm_opm_destroy(YmfmOpmChip* chip) {
    delete chip;
}

void ymfm_opm_reset(YmfmOpmChip* chip) {
    chip->chip.reset();
}

void ymfm_opm_write(YmfmOpmChip* chip, uint8_t addr, uint8_t data) {
    chip->chip.write_address(addr);
    chip->chip.write_data(data);
}

void ymfm_opm_generate(YmfmOpmChip* chip, int32_t* left, int32_t* right) {
    ymfm::ym2151::output_data output;
    chip->chip.generate(&output);
    *left = output.data[0];
    *right = output.data[1];
}

uint32_t ymfm_opm_sample_rate(uint32_t clock) {
    return clock / 64; // YM2151 divides master clock by 64
}

} // extern "C"
