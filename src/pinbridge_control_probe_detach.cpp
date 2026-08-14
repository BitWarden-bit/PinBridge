#include "pinbridge/pinbridge.h"

#include "control_probe_detach_backend.h"

namespace
{

template< typename Function > PbStatus GuardProbeDetach(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_add_detach_function_probed(
    PbDetachProbedCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    if (out_callback)
        out_callback->opaque = 0;
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardProbeDetach([&]() {
        return PbBackendAddDetachFunctionProbed(
            callback, user_data, &out_callback->opaque);
    });
}

PbStatus PB_CALL pb_pin_detach_probed(void)
{
    return GuardProbeDetach([]() { return PbBackendDetachProbed(); });
}

PbStatus PB_CALL pb_pin_detach(void)
{
    return GuardProbeDetach([]() { return PbBackendDetach(); });
}
