#include "pin.H"

#include "physical_context_backend.h"

#include <cstring>

namespace
{

static_assert(sizeof(FXSAVE) == PB_FXSAVE_SIZE,
              "PbFxSave size differs from this Pin SDK");
static_assert(FPSTATE_ALIGNMENT == 64, "unexpected Pin FPSTATE alignment");

bool InProbeMode()
{
    return PIN_IsProbeMode() != 0;
}

} // namespace

PbStatus PbBackendGetPhysicalContextReg(
    const void* context, PbRegId reg, uint64_t* out_value)
{
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_value = static_cast<uint64_t>(PIN_GetPhysicalContextReg(
        static_cast<const PHYSICAL_CONTEXT*>(context), static_cast<REG>(reg)));
    return PB_OK;
}

PbStatus PbBackendSetPhysicalContextReg(
    void* context, PbRegId reg, uint64_t value)
{
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    PIN_SetPhysicalContextReg(
        static_cast<PHYSICAL_CONTEXT*>(context), static_cast<REG>(reg),
        static_cast<ADDRINT>(value));
    return PB_OK;
}

PbStatus PbBackendGetPhysicalContextFxSave(
    const void* context, PbFxSave* out_fxsave)
{
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    alignas(FPSTATE_ALIGNMENT) FPSTATE state = {};
    PIN_GetPhysicalContextFPState(
        static_cast<const PHYSICAL_CONTEXT*>(context), &state);
    std::memcpy(out_fxsave->bytes, &state, PB_FXSAVE_SIZE);
    return PB_OK;
}

PbStatus PbBackendSetPhysicalContextFxSave(
    void* context, const PbFxSave* fxsave)
{
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    alignas(FPSTATE_ALIGNMENT) FPSTATE state = {};
    std::memcpy(&state, fxsave->bytes, PB_FXSAVE_SIZE);
    PIN_SetPhysicalContextFPState(static_cast<PHYSICAL_CONTEXT*>(context), &state);
    return PB_OK;
}
