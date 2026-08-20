#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_calls;

static PbExceptHandlingResult PB_CALL OnInternalException(
    PbThreadId thread_id, PbExceptionInfoHandle exception_info,
    PbPhysicalContextHandle physical_context, void* user_data)
{
    if (thread_id != 7u ||
        exception_info != (PbExceptionInfoHandle)(uintptr_t)UINT64_C(0x4000) ||
        physical_context != (PbPhysicalContextHandle)(uintptr_t)UINT64_C(0x5000) ||
        user_data != &g_calls)
        return PB_EHR_UNHANDLED;
    ++g_calls;
    return PB_EHR_HANDLED;
}

int main(void)
{
    PbCallbackHandle callback = {UINT64_C(99)};
    PbCallbackHandle scope = {UINT64_C(99)};

    if (pb_pin_add_internal_exception_handler(OnInternalException, &g_calls, &callback) !=
            PB_OK ||
        callback.opaque != UINT64_C(0x3201) || g_calls != 1u)
        return 1;
    if (pb_pin_enable_single_step_passthrough(&callback) != PB_OK ||
        callback.opaque != UINT64_C(0x3203))
        return 6;
    if (pb_pin_set_single_step_passthrough(7u, 1u) != PB_OK ||
        pb_pin_set_single_step_passthrough(7u, 0u) != PB_OK ||
        pb_pin_set_single_step_passthrough(8u, 1u) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_single_step_passthrough(7u, 2u) != PB_ERR_INVALID_ARGUMENT)
        return 7;
    if (pb_pin_try_start(7u, OnInternalException, &g_calls, &scope) != PB_OK ||
        scope.opaque != UINT64_C(0x3202) || g_calls != 2u)
        return 2;
    if (pb_pin_try_end(7u, &scope) != PB_OK || scope.opaque != 0)
        return 3;
    if (pb_pin_add_internal_exception_handler(0, 0, &callback) !=
            PB_ERR_INVALID_ARGUMENT ||
        callback.opaque != 0 ||
        pb_pin_add_internal_exception_handler(OnInternalException, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_enable_single_step_passthrough(0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    if (pb_pin_try_start(7u, 0, 0, &scope) != PB_ERR_INVALID_ARGUMENT ||
        scope.opaque != 0 ||
        pb_pin_try_start(7u, OnInternalException, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_try_end(7u, &scope) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_try_end(7u, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
