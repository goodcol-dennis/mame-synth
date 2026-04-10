#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct YmfmChip YmfmChip;

YmfmChip* ymfm_ym2612_create(uint32_t clock_rate);
void ymfm_ym2612_destroy(YmfmChip* chip);
void ymfm_ym2612_reset(YmfmChip* chip);
void ymfm_ym2612_write(YmfmChip* chip, uint8_t port, uint8_t addr, uint8_t data);
void ymfm_ym2612_generate(YmfmChip* chip, int32_t* left, int32_t* right);
uint32_t ymfm_ym2612_sample_rate(uint32_t clock);

#ifdef __cplusplus
}
#endif
