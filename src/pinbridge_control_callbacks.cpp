#include "pinbridge/pinbridge.h"

#include "control_callback_backend.h"

namespace
{

template< typename Function > PbStatus GuardRegistration(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

template< typename Callback, typename Register > PbStatus AddCallback(
    Callback callback, void* user_data, PbCallbackHandle* out_callback, Register registration)
{
    if (out_callback)
        out_callback->opaque = 0;
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRegistration([&]() -> PbStatus {
        return registration(callback, user_data, &out_callback->opaque);
    });
}

} // namespace

PbStatus PB_CALL pb_pin_add_application_start_function(
    PbApplicationStartCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddApplicationStartFunction);
}

PbStatus PB_CALL pb_pin_add_prepare_for_fini_function(
    PbPrepareForFiniCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddPrepareForFiniFunction);
}

PbStatus PB_CALL pb_pin_add_fini_function(
    PbFiniCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddFiniFunction);
}

PbStatus PB_CALL pb_pin_add_thread_start_function(
    PbThreadStartCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddThreadStartFunction);
}

PbStatus PB_CALL pb_pin_add_thread_fini_function(
    PbThreadFiniCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddThreadFiniFunction);
}

PbStatus PB_CALL pb_pin_add_context_change_function(
    PbContextChangeCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddContextChangeFunction);
}

PbStatus PB_CALL pb_pin_add_xed_decode_callback_function(
    PbXedDecodeCallback callback, void* user_data)
{
    if (!callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRegistration([&]() -> PbStatus {
        return PbBackendAddXedDecodeCallbackFunction(callback, user_data);
    });
}
