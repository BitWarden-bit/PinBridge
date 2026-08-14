#include "pinbridge/pinbridge.h"

#include "structure_callback_backend.h"

namespace
{

template< typename Function > PbStatus GuardCallbackRegistration(Function function)
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
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    out_callback->opaque = 0;
    return GuardCallbackRegistration([&]() -> PbStatus {
        return registration(callback, user_data, &out_callback->opaque);
    });
}

} // namespace

PbStatus PB_CALL pb_trace_add_instrument_function(
    PbTraceInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddTraceInstrumentFunction);
}

PbStatus PB_CALL pb_rtn_add_instrument_function(
    PbRtnInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddRtnInstrumentFunction);
}

PbStatus PB_CALL pb_img_add_instrument_function(
    PbImgInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback, PbBackendAddImgInstrumentFunction);
}
