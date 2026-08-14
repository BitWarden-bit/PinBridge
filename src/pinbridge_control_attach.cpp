#include "pinbridge/pinbridge.h"

#include "control_attach_backend.h"

namespace
{

template< typename Function > PbStatus GuardAttach(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

template< typename Callback, typename Attach > PbStatus RequestAttach(
    Callback callback, void* user_data, PbAttachStatus* out_status, Attach attach)
{
    if (out_status)
        *out_status = PB_ATTACH_FAILED_DETACH;
    if (!callback || !out_status)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardAttach([&]() { return attach(callback, user_data, out_status); });
}

} // namespace

PbStatus PB_CALL pb_pin_attach_probed(
    PbAttachProbedCallback callback, void* user_data, PbAttachStatus* out_status)
{
    return RequestAttach(callback, user_data, out_status, PbBackendAttachProbed);
}
