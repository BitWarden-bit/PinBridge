#include "trace_instrumentation_backend.h"

namespace
{

uint64_t g_predicate_result;

} // namespace

PbStatus PbBackendTraceInsertCallBefore(
    PbTraceHandle, PbTraceAnalysisCallback callback, void* user_data)
{
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendTraceInsertIfCallBefore(
    PbTraceHandle, PbTracePredicateCallback callback, void* user_data)
{
    g_predicate_result = callback(user_data);
    return PB_OK;
}

PbStatus PbBackendTraceInsertThenCallBefore(
    PbTraceHandle, PbTraceAnalysisCallback callback, void* user_data)
{
    if (g_predicate_result != 0)
        callback(user_data);
    return PB_OK;
}
