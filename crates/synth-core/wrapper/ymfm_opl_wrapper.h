#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// YM3812 (OPL2)
typedef struct YmfmOpl2Chip YmfmOpl2Chip;
YmfmOpl2Chip* ymfm_opl2_create(uint32_t clock_rate);
void ymfm_opl2_destroy(YmfmOpl2Chip* chip);
void ymfm_opl2_reset(YmfmOpl2Chip* chip);
void ymfm_opl2_write(YmfmOpl2Chip* chip, uint8_t addr, uint8_t data);
void ymfm_opl2_generate(YmfmOpl2Chip* chip, int32_t* left, int32_t* right);
uint32_t ymfm_opl2_sample_rate(uint32_t clock);

// YMF262 (OPL3)
typedef struct YmfmOpl3Chip YmfmOpl3Chip;
YmfmOpl3Chip* ymfm_opl3_create(uint32_t clock_rate);
void ymfm_opl3_destroy(YmfmOpl3Chip* chip);
void ymfm_opl3_reset(YmfmOpl3Chip* chip);
void ymfm_opl3_write(YmfmOpl3Chip* chip, uint8_t port, uint8_t addr, uint8_t data);
void ymfm_opl3_generate(YmfmOpl3Chip* chip, int32_t* out0, int32_t* out1, int32_t* out2, int32_t* out3);
uint32_t ymfm_opl3_sample_rate(uint32_t clock);

#ifdef __cplusplus
}
#endif
