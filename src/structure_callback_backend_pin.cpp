#include "pin.H"

#include "structure_callback_backend.h"

#include <cstdlib>

namespace
{

template< typename Callback > struct CallbackState
{
    Callback callback;
    void* user_data;
};

VOID OnTrace(TRACE trace, VOID* raw_state)
{
    CallbackState<PbTraceInstrumentCallback>* state =
        static_cast<CallbackState<PbTraceInstrumentCallback>*>(raw_state);
    state->callback(reinterpret_cast<PbTraceHandle>(trace), state->user_data);
}

VOID OnRtn(RTN rtn, VOID* raw_state)
{
    CallbackState<PbRtnInstrumentCallback>* state =
        static_cast<CallbackState<PbRtnInstrumentCallback>*>(raw_state);
    const PbRtnHandle handle = {rtn.q()};
    state->callback(handle, state->user_data);
}

VOID OnImg(IMG img, VOID* raw_state)
{
    CallbackState<PbImgInstrumentCallback>* state =
        static_cast<CallbackState<PbImgInstrumentCallback>*>(raw_state);
    const PbImgHandle handle = {img.q()};
    state->callback(handle, state->user_data);
}

template< typename Callback, typename Register > PbStatus AddCallback(
    Callback callback, void* user_data, uint64_t* out_callback, Register registration)
{
    CallbackState<Callback>* state =
        static_cast<CallbackState<Callback>*>(std::malloc(sizeof(CallbackState<Callback>)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK pin_callback = registration(state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

} // namespace

PbStatus PbBackendAddTraceInstrumentFunction(
    PbTraceInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
                       [](void* state) { return TRACE_AddInstrumentFunction(OnTrace, state); });
}

PbStatus PbBackendAddRtnInstrumentFunction(
    PbRtnInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
                       [](void* state) { return RTN_AddInstrumentFunction(OnRtn, state); });
}

PbStatus PbBackendAddImgInstrumentFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
                       [](void* state) { return IMG_AddInstrumentFunction(OnImg, state); });
}
