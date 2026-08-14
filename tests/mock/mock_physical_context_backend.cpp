#include "physical_context_backend.h"

#include <cstring>

namespace
{

const uint32_t kFxSaveOffset = 32;

} // namespace

PbStatus PbBackendGetPhysicalContextReg(
    const void* context, PbRegId reg, uint64_t* out_value)
{
    const uint64_t* slots = static_cast<const uint64_t*>(context);
    *out_value = slots[reg - PB_REG_PHYSICAL_INTEGER_BASE];
    return PB_OK;
}

PbStatus PbBackendSetPhysicalContextReg(
    void* context, PbRegId reg, uint64_t value)
{
    uint64_t* slots = static_cast<uint64_t*>(context);
    slots[reg - PB_REG_PHYSICAL_INTEGER_BASE] = value;
    return PB_OK;
}

PbStatus PbBackendGetPhysicalContextFxSave(
    const void* context, PbFxSave* out_fxsave)
{
    std::memcpy(
        out_fxsave->bytes, static_cast<const uint8_t*>(context) + kFxSaveOffset,
        PB_FXSAVE_SIZE);
    return PB_OK;
}

PbStatus PbBackendSetPhysicalContextFxSave(
    void* context, const PbFxSave* fxsave)
{
    std::memcpy(
        static_cast<uint8_t*>(context) + kFxSaveOffset, fxsave->bytes,
        PB_FXSAVE_SIZE);
    return PB_OK;
}
