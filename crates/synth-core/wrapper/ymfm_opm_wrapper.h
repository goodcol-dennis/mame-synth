#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct YmfmOpmChip YmfmOpmChip;

YmfmOpmChip* ymfm_opm_create(uint32_t clock_rate);
void ymfm_opm_destroy(YmfmOpmChip* chip);
void ymfm_opm_reset(YmfmOpmChip* chip);
void ymfm_opm_write(YmfmOpmChip* chip, uint8_t addr, uint8_t data);
void ymfm_opm_generate(YmfmOpmChip* chip, int32_t* left, int32_t* right);
uint32_t ymfm_opm_sample_rate(uint32_t clock);

#ifdef __cplusplus
}
#endif
