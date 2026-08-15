#include "pin.H"

#include "reg_function_backend.h"
#include "reg_mapping_pin.h"

#include <cstring>

uint32_t PbBackendClaimToolRegister(void)
{
    return static_cast<uint32_t>(PIN_ClaimToolRegister());
}

uint16_t PbBackendConvertX87AbridgedTagToFull(const uint8_t* fxsave_bytes)
{
    alignas(64) FXSAVE fxsave = {};
    std::memcpy(&fxsave, fxsave_bytes, sizeof(fxsave));
    return REG_ConvertX87AbridgedTagToFull(&fxsave);
}

uint64_t PbBackendRegStringShort(uint32_t reg, char* buffer, uint64_t capacity)
{
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg))
        return 0;
    const std::string value = REG_StringShort(native_reg);
    const uint64_t required = static_cast<uint64_t>(value.size()) + 1u;
    if (buffer && capacity >= required)
        std::memcpy(buffer, value.c_str(), static_cast<size_t>(required));
    return required;
}
