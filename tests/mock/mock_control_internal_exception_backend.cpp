#include "control_internal_exception_backend.h"

namespace
{

uint64_t g_scope = UINT64_C(0x3202);

} // namespace

PbStatus PbBackendAddInternalExceptionHandler(
    PbInternalExceptionCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3201);
    callback(7u,
        reinterpret_cast<PbExceptionInfoHandle>(static_cast<uintptr_t>(0x4000)),
        reinterpret_cast<PbPhysicalContextHandle>(static_cast<uintptr_t>(0x5000)),
        user_data);
    return PB_OK;
}

PbStatus PbBackendEnableSingleStepPassthrough(uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3203);
    return PB_OK;
}

PbStatus PbBackendSetSingleStepPassthrough(PbThreadId thread_id, uint8_t enabled)
{
    return (thread_id == 7u && enabled <= 1u) ? PB_OK : PB_ERR_INVALID_ARGUMENT;
}

PbStatus PbBackendTryStart(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    uint64_t* out_scope)
{
    *out_scope = g_scope;
    callback(thread_id,
        reinterpret_cast<PbExceptionInfoHandle>(static_cast<uintptr_t>(0x4000)),
        reinterpret_cast<PbPhysicalContextHandle>(static_cast<uintptr_t>(0x5000)),
        user_data);
    return PB_OK;
}

PbStatus PbBackendTryEnd(PbThreadId thread_id, uint64_t scope)
{
    if (thread_id != 7u || scope != g_scope)
        return PB_ERR_INVALID_STATE;
    return PB_OK;
}
