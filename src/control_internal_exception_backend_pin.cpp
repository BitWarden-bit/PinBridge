#include "pin.H"

#include "control_internal_exception_backend.h"

#include <cstdlib>

namespace
{

struct CallbackState
{
    PbInternalExceptionCallback callback;
    void* user_data;
    PbThreadId thread_id;
};

EXCEPT_HANDLING_RESULT OnInternalException(
    THREADID thread_id, EXCEPTION_INFO* exception_info,
    PHYSICAL_CONTEXT* physical_context, VOID* raw_state)
{
    CallbackState* state = static_cast<CallbackState*>(raw_state);
    const PbExceptHandlingResult result = state->callback(
        static_cast<PbThreadId>(thread_id),
        reinterpret_cast<PbExceptionInfoHandle>(exception_info),
        reinterpret_cast<PbPhysicalContextHandle>(physical_context),
        state->user_data);
    if (result != PB_EHR_HANDLED && result != PB_EHR_UNHANDLED &&
        result != PB_EHR_CONTINUE_SEARCH)
        return EHR_UNHANDLED;
    return static_cast<EXCEPT_HANDLING_RESULT>(result);
}

CallbackState* AllocateState(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data)
{
    CallbackState* state = static_cast<CallbackState*>(std::malloc(sizeof(CallbackState)));
    if (state)
    {
        state->callback = callback;
        state->user_data = user_data;
        state->thread_id = thread_id;
    }
    return state;
}

} // namespace

PbStatus PbBackendAddInternalExceptionHandler(
    PbInternalExceptionCallback callback, void* user_data, uint64_t* out_callback)
{
    CallbackState* state = AllocateState(0, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const PIN_CALLBACK pin_callback = PIN_AddInternalExceptionHandler(OnInternalException, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

PbStatus PbBackendTryStart(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    uint64_t* out_scope)
{
    CallbackState* state = AllocateState(thread_id, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    PIN_TryStart(static_cast<THREADID>(thread_id), OnInternalException, state);
    *out_scope = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(state));
    return PB_OK;
}

PbStatus PbBackendTryEnd(PbThreadId thread_id, uint64_t scope)
{
    CallbackState* state = reinterpret_cast<CallbackState*>(static_cast<uintptr_t>(scope));
    if (state->thread_id != thread_id)
        return PB_ERR_INVALID_STATE;
    PIN_TryEnd(static_cast<THREADID>(thread_id));
    std::free(state);
    return PB_OK;
}
