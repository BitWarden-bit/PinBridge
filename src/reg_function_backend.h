#ifndef PINBRIDGE_REG_FUNCTION_BACKEND_H
#define PINBRIDGE_REG_FUNCTION_BACKEND_H

#include <stdint.h>

uint32_t PbBackendClaimToolRegister(void);
uint16_t PbBackendConvertX87AbridgedTagToFull(const uint8_t* fxsave_bytes);
uint64_t PbBackendRegStringShort(uint32_t reg, char* buffer, uint64_t capacity);

#endif
