#include "pin.H"

#include "control_query_backend.h"

PbStatus PbBackendCheckReadAccess(uint64_t address, uint8_t* out_accessible)
{
    *out_accessible = PIN_CheckReadAccess(
        reinterpret_cast<VOID*>(static_cast<uintptr_t>(address))) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendCheckWriteAccess(uint64_t address, uint8_t* out_accessible)
{
    *out_accessible = PIN_CheckWriteAccess(
        reinterpret_cast<VOID*>(static_cast<uintptr_t>(address))) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendIsAttaching(uint8_t* out_attaching)
{
    *out_attaching = PIN_IsAttaching() ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendIsProbeMode(uint8_t* out_probe_mode)
{
    *out_probe_mode = PIN_IsProbeMode() ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendIsSafeForProbedInsertion(uint64_t address, uint8_t* out_safe)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_safe = PIN_IsSafeForProbedInsertion(static_cast<ADDRINT>(address)) ? 1u : 0u;
    return PB_OK;
}

const char* PbBackendToolFullPath(void) { return PIN_ToolFullPath(); }
