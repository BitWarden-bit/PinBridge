#include "pinbridge/pinbridge.h"

#include "trace_smc_backend.h"

PbStatus PB_CALL pb_trace_add_smc_detected_function(
    PbTraceSmcCallback callback, void* user_data)
{
    if (!callback)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return PbBackendTraceAddSmcDetectedFunction(callback, user_data);
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return PbBackendTraceAddSmcDetectedFunction(callback, user_data);
#endif
}
