#include "bbl_instrumentation_backend.h"

namespace
{

uint64_t g_predicate_result;

} // namespace

PbStatus PbBackendBblInsertCallBefore(
    PbBblHandle, PbBblAnalysisCallback callback, void* user_data)
{
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendBblInsertIfCallBefore(
    PbBblHandle, PbBblPredicateCallback callback, void* user_data)
{
    g_predicate_result = callback(user_data);
    return PB_OK;
}

PbStatus PbBackendBblInsertThenCallBefore(
    PbBblHandle, PbBblAnalysisCallback callback, void* user_data)
{
    if (g_predicate_result != 0)
        callback(user_data);
    return PB_OK;
}
