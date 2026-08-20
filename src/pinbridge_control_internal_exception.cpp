#include "pinbridge/pinbridge.h"

#include "control_internal_exception_backend.h"

namespace
{

template< typename Function > PbStatus GuardInternalException(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_add_internal_exception_handler(
    PbInternalExceptionCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    if (out_callback)
        out_callback->opaque = 0;
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInternalException([&]() {
        return PbBackendAddInternalExceptionHandler(
            callback, user_data, &out_callback->opaque);
    });
}

PbStatus PB_CALL pb_pin_enable_single_step_passthrough(
    PbCallbackHandle* out_callback)
{
    if (out_callback)
        out_callback->opaque = 0;
    if (!out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInternalException([&]() {
        return PbBackendEnableSingleStepPassthrough(&out_callback->opaque);
    });
}

PbStatus PB_CALL pb_pin_set_single_step_passthrough(
    PbThreadId thread_id, uint8_t enabled)
{
    if (enabled > 1u)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInternalException([&]() {
        return PbBackendSetSingleStepPassthrough(thread_id, enabled);
    });
}

PbStatus PB_CALL pb_pin_try_start(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    PbCallbackHandle* out_scope)
{
    if (out_scope)
        out_scope->opaque = 0;
    if (!callback || !out_scope)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInternalException([&]() {
        return PbBackendTryStart(thread_id, callback, user_data, &out_scope->opaque);
    });
}

PbStatus PB_CALL pb_pin_try_end(PbThreadId thread_id, PbCallbackHandle* scope)
{
    if (!scope || scope->opaque == 0)
        return PB_ERR_INVALID_ARGUMENT;
    const PbStatus status = GuardInternalException(
        [&]() { return PbBackendTryEnd(thread_id, scope->opaque); });
    if (status == PB_OK)
        scope->opaque = 0;
    return status;
}
