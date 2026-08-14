#include "pinbridge/pinbridge.h"

#include "callbacks_backend.h"

namespace
{

template< typename Function > PbStatus GuardCallbackOperation(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_callback_get_execution_order(
    PbCallbackHandle callback, PbCallOrder* out_order)
{
    if (!out_order || callback.opaque == PB_CALLBACK_INVALID_OPAQUE)
        return PB_ERR_INVALID_ARGUMENT;
    *out_order = 0;
    return GuardCallbackOperation(
        [&]() { return PbBackendCallbackGetExecutionOrder(callback, out_order); });
}

PbStatus PB_CALL pb_callback_set_execution_order(
    PbCallbackHandle callback, PbCallOrder order)
{
    if (callback.opaque == PB_CALLBACK_INVALID_OPAQUE)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardCallbackOperation(
        [&]() { return PbBackendCallbackSetExecutionOrder(callback, order); });
}
