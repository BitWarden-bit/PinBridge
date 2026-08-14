#include "trace_smc_backend.h"

PbStatus PbBackendTraceAddSmcDetectedFunction(
    PbTraceSmcCallback callback, void* user_data)
{
    callback(UINT64_C(0x1000), UINT64_C(0x1010), user_data);
    return PB_OK;
}
