#include "pin.H"

#include "control_attach_backend.h"

#include <cstdlib>

namespace
{

template< typename Callback > struct CallbackState
{
    Callback callback;
    void* user_data;
};

VOID OnAttachProbed(VOID* raw_state)
{
    CallbackState<PbAttachProbedCallback>* state =
        static_cast<CallbackState<PbAttachProbedCallback>*>(raw_state);
    PbAttachProbedCallback callback = state->callback;
    void* user_data = state->user_data;
    std::free(state);
    callback(user_data);
}

VOID OnAttach(VOID* raw_state)
{
    CallbackState<PbAttachCallback>* state =
        static_cast<CallbackState<PbAttachCallback>*>(raw_state);
    PbAttachCallback callback = state->callback;
    void* user_data = state->user_data;
    std::free(state);
    callback(user_data);
}

template< typename Callback, typename Attach > PbStatus RequestAttach(
    Callback callback, void* user_data, PbAttachStatus* out_status, Attach attach)
{
    CallbackState<Callback>* state =
        static_cast<CallbackState<Callback>*>(std::malloc(sizeof(CallbackState<Callback>)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const ATTACH_STATUS status = attach(state);
    *out_status = static_cast<PbAttachStatus>(status);
    if (status == ATTACH_FAILED_DETACH)
        std::free(state);
    return PB_OK;
}

} // namespace

PbStatus PbBackendAttach(
    PbAttachCallback callback, void* user_data, PbAttachStatus* out_status)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
#if defined(TARGET_WINDOWS)
    // Pin 3.31 exports PIN_Attach on Windows but terminates the target with
    // "Re-Attach ... is NYI" when it is called in JIT mode. Reject at the
    // bridge boundary so a capability mistake cannot kill the application.
    (void)callback;
    (void)user_data;
    (void)out_status;
    return PB_ERR_UNSUPPORTED;
#else
    return RequestAttach(callback, user_data, out_status,
        [](void* state) { return PIN_Attach(OnAttach, state); });
#endif
}

PbStatus PbBackendAttachProbed(
    PbAttachProbedCallback callback, void* user_data, PbAttachStatus* out_status)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    return RequestAttach(callback, user_data, out_status,
        [](void* state) { return PIN_AttachProbed(OnAttachProbed, state); });
}
