#include "ymfm_wrapper.h"
#include "../ymfm/src/ymfm_opn.h"

// Minimal interface implementation required by YMFM.
// All virtual methods have sensible defaults; we only need the timer_set callback
// to avoid pure-virtual calls.
class WrapperInterface : public ymfm::ymfm_interface {
public:
    // YMFM calls this to set timers; we don't need timer functionality for a synth.
    void ymfm_sync_mode_write(uint8_t) override {}
    void ymfm_sync_check_interrupts() override {}
    void ymfm_set_timer(uint32_t, int32_t) override {}
    void ymfm_set_busy_end(uint32_t) override {}
    bool ymfm_is_busy() override { return false; }
};

struct YmfmChip {
    WrapperInterface interface;
    ymfm::ym2612 chip;

    YmfmChip(uint32_t clock)
        : interface()
        , chip(interface)
    {
        chip.reset();
    }
};

extern "C" {

YmfmChip* ymfm_ym2612_create(uint32_t clock_rate) {
    return new YmfmChip(clock_rate);
}

void ymfm_ym2612_destroy(YmfmChip* chip) {
    delete chip;
}

void ymfm_ym2612_reset(YmfmChip* chip) {
    chip->chip.reset();
}

void ymfm_ym2612_write(YmfmChip* chip, uint8_t port, uint8_t addr, uint8_t data) {
    // Port 0 = registers 0x00-0xFF (channels 1-3)
    // Port 1 = registers 0x00-0xFF (channels 4-6)
    uint32_t offset = port * 2; // address port offset
    chip->chip.write(offset, addr);     // write address
    chip->chip.write(offset + 1, data); // write data
}

void ymfm_ym2612_generate(YmfmChip* chip, int32_t* left, int32_t* right) {
    ymfm::ym2612::output_data output;
    chip->chip.generate(&output);
    // YM2612 has 2 output channels (left/right)
    *left = output.data[0];
    *right = output.data[1];
}

uint32_t ymfm_ym2612_sample_rate(uint32_t clock) {
    return clock / 144; // YM2612 divides master clock by 144
}

} // extern "C"
