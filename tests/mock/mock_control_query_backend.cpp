#include "control_query_backend.h"

PbStatus PbBackendCheckReadAccess(uint64_t, uint8_t* out_accessible)
{
    *out_accessible = 1u;
    return PB_OK;
}

PbStatus PbBackendCheckWriteAccess(uint64_t, uint8_t* out_accessible)
{
    *out_accessible = 0u;
    return PB_OK;
}

PbStatus PbBackendIsAttaching(uint8_t* out_attaching)
{
    *out_attaching = 0u;
    return PB_OK;
}

PbStatus PbBackendIsProbeMode(uint8_t* out_probe_mode)
{
    *out_probe_mode = 0u;
    return PB_OK;
}

PbStatus PbBackendIsSafeForProbedInsertion(uint64_t, uint8_t*)
{
    return PB_ERR_INVALID_STATE;
}

const char* PbBackendToolFullPath(void) { return "C:\\mock\\pinbridge.dll"; }
