#include "pin.H"

#include "control_probe_detach_backend.h"

#include <cstdlib>

namespace
{

template< typename Callback > struct CallbackState
{
    Callback callback;
    void* user_data;
};

VOID OnDetach(VOID* raw_state)
{
    CallbackState<PbDetachCallback>* state =
        static_cast<CallbackState<PbDetachCallback>*>(raw_state);
    PbDetachCallback callback = state->callback;
    void* user_data = state->user_data;
    std::free(state);
    callback(user_data);
}

VOID OnDetachProbed(VOID* raw_state)
{
    CallbackState<PbDetachProbedCallback>* state =
        static_cast<CallbackState<PbDetachProbedCallback>*>(raw_state);
    PbDetachProbedCallback callback = state->callback;
    void* user_data = state->user_data;
    std::free(state);
    callback(user_data);
}

} // namespace

PbStatus PbBackendAddDetachFunction(
    PbDetachCallback callback, void* user_data, uint64_t* out_callback)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    CallbackState<PbDetachCallback>* state =
        static_cast<CallbackState<PbDetachCallback>*>(
            std::malloc(sizeof(CallbackState<PbDetachCallback>)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK pin_callback = PIN_AddDetachFunction(OnDetach, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

PbStatus PbBackendAddDetachFunctionProbed(
    PbDetachProbedCallback callback, void* user_data, uint64_t* out_callback)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    CallbackState<PbDetachProbedCallback>* state =
        static_cast<CallbackState<PbDetachProbedCallback>*>(
            std::malloc(sizeof(CallbackState<PbDetachProbedCallback>)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK pin_callback = PIN_AddDetachFunctionProbed(OnDetachProbed, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

PbStatus PbBackendDetachProbed(void)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PIN_DetachProbed();
    return PB_OK;
}

PbStatus PbBackendDetach(void)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PIN_Detach();
    return PB_OK;
}
