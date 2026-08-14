#include "ins_instrumentation_backend.h"

namespace
{
uint64_t g_predicate_result;
}

PbStatus PbBackendInsInsertCallBefore(
    PbInsHandle, PbInsAnalysisCallback callback, void* user_data)
{
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertCallBeforeCtx(
    PbInsHandle, PbInsContextAnalysisCallback callback, void* user_data)
{
    callback(0, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfCallBefore(
    PbInsHandle, PbInsPredicateCallback callback, void* user_data)
{
    g_predicate_result = callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertThenCallBefore(
    PbInsHandle, PbInsAnalysisCallback callback, void* user_data)
{
    if (g_predicate_result != 0)
        callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertPredicatedCallBefore(
    PbInsHandle, PbInsAnalysisCallback callback, void* user_data)
{
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfPredicatedCallBefore(
    PbInsHandle, PbInsPredicateCallback callback, void* user_data)
{
    g_predicate_result = callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertThenPredicatedCallBefore(
    PbInsHandle, PbInsAnalysisCallback callback, void* user_data)
{
    if (g_predicate_result != 0)
        callback(user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBuffer(
    PbInsHandle, PbIpoint, PbBufferId, uint32_t)
{
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBufferPredicated(
    PbInsHandle, PbIpoint, PbBufferId, uint32_t)
{
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBufferThen(
    PbInsHandle, PbIpoint, PbBufferId, uint32_t)
{
    return PB_OK;
}
