#include "pin.H"

#include "control_probe_detach_backend.h"

#include <cstdlib>

namespace
{

struct CallbackState
{
    PbDetachProbedCallback callback;
    void* user_data;
};

VOID OnDetachProbed(VOID* raw_state)
{
    CallbackState* state = static_cast<CallbackState*>(raw_state);
    PbDetachProbedCallback callback = state->callback;
    void* user_data = state->user_data;
    std::free(state);
    callback(user_data);
}

} // namespace

PbStatus PbBackendAddDetachFunctionProbed(
    PbDetachProbedCallback callback, void* user_data, uint64_t* out_callback)
{
    CallbackState* state = static_cast<CallbackState*>(std::malloc(sizeof(CallbackState)));
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
