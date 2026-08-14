#include "pinbridge/pinbridge.h"

#include "reg_function_backend.h"

#include <cstring>

uint32_t PbBackendClaimToolRegister(void)
{
    return PB_REG_INST_G0;
}

uint16_t PbBackendConvertX87AbridgedTagToFull(const uint8_t* fxsave_bytes)
{
    return static_cast<uint16_t>(UINT16_C(0x1200) | fxsave_bytes[4]);
}

uint64_t PbBackendRegStringShort(uint32_t, char* buffer, uint64_t capacity)
{
    static const char value[] = "mock-reg";
    const uint64_t required = sizeof(value);
    if (buffer && capacity >= required)
        std::memcpy(buffer, value, sizeof(value));
    return required;
}
