#include "pin.H"

#include "control_callback_backend.h"

#include <cstdlib>

namespace
{

template< typename Callback > struct CallbackState
{
    Callback callback;
    void* user_data;
};

CallbackState<PbXedDecodeCallback> g_xed_decode_state = {};
bool g_xed_decode_registered;

VOID OnApplicationStart(VOID* raw_state)
{
    CallbackState<PbApplicationStartCallback>* state =
        static_cast<CallbackState<PbApplicationStartCallback>*>(raw_state);
    state->callback(state->user_data);
}

VOID OnPrepareForFini(VOID* raw_state)
{
    CallbackState<PbPrepareForFiniCallback>* state =
        static_cast<CallbackState<PbPrepareForFiniCallback>*>(raw_state);
    state->callback(state->user_data);
}

VOID OnFini(INT32 code, VOID* raw_state)
{
    CallbackState<PbFiniCallback>* state =
        static_cast<CallbackState<PbFiniCallback>*>(raw_state);
    state->callback(static_cast<int32_t>(code), state->user_data);
}

VOID OnThreadStart(THREADID thread_id, CONTEXT* context, INT32 flags, VOID* raw_state)
{
    CallbackState<PbThreadStartCallback>* state =
        static_cast<CallbackState<PbThreadStartCallback>*>(raw_state);
    state->callback(static_cast<PbThreadId>(thread_id),
        reinterpret_cast<PbContextHandle>(context), static_cast<int32_t>(flags),
        state->user_data);
}

VOID OnThreadFini(THREADID thread_id, const CONTEXT* context, INT32 code, VOID* raw_state)
{
    CallbackState<PbThreadFiniCallback>* state =
        static_cast<CallbackState<PbThreadFiniCallback>*>(raw_state);
    state->callback(static_cast<PbThreadId>(thread_id),
        reinterpret_cast<PbConstContextHandle>(context), static_cast<int32_t>(code),
        state->user_data);
}

VOID OnContextChange(THREADID thread_id, CONTEXT_CHANGE_REASON reason,
    const CONTEXT* from, CONTEXT* to, INT32 info, VOID* raw_state)
{
    CallbackState<PbContextChangeCallback>* state =
        static_cast<CallbackState<PbContextChangeCallback>*>(raw_state);
    state->callback(static_cast<PbThreadId>(thread_id),
        static_cast<PbContextChangeReason>(reason),
        reinterpret_cast<PbConstContextHandle>(from),
        reinterpret_cast<PbContextHandle>(to), static_cast<int32_t>(info),
        state->user_data);
}

VOID OnXedDecode(xed_decoded_inst_t* decoded_instruction)
{
    g_xed_decode_state.callback(
        reinterpret_cast<PbXedDecodedInstHandle>(decoded_instruction),
        g_xed_decode_state.user_data);
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

PbStatus PbBackendAddApplicationStartFunction(
    PbApplicationStartCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddApplicationStartFunction(OnApplicationStart, state); });
}

PbStatus PbBackendAddPrepareForFiniFunction(
    PbPrepareForFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddPrepareForFiniFunction(OnPrepareForFini, state); });
}

PbStatus PbBackendAddFiniFunction(
    PbFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddFiniFunction(OnFini, state); });
}

PbStatus PbBackendAddThreadStartFunction(
    PbThreadStartCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddThreadStartFunction(OnThreadStart, state); });
}

PbStatus PbBackendAddThreadFiniFunction(
    PbThreadFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddThreadFiniFunction(OnThreadFini, state); });
}

PbStatus PbBackendAddContextChangeFunction(
    PbContextChangeCallback callback, void* user_data, uint64_t* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        [](void* state) { return PIN_AddContextChangeFunction(OnContextChange, state); });
}

PbStatus PbBackendAddXedDecodeCallbackFunction(
    PbXedDecodeCallback callback, void* user_data)
{
    g_xed_decode_state.callback = callback;
    g_xed_decode_state.user_data = user_data;
    if (!g_xed_decode_registered)
    {
        PIN_AddXedDecodeCallbackFunction(OnXedDecode);
        g_xed_decode_registered = true;
    }
    return PB_OK;
}
