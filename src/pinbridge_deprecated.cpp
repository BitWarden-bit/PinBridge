#include "pinbridge/pinbridge.h"

#include "deprecated_backend.h"

namespace
{

template< typename Function > PbStatus GuardDeprecatedOperation(Function function)
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

PbStatus PB_CALL pb_callback_get_execution_priority_deprecated(
    PbCallbackHandle callback, int32_t* out_priority)
{
    if (!out_priority || callback.opaque == PB_CALLBACK_INVALID_OPAQUE)
        return PB_ERR_INVALID_ARGUMENT;
    *out_priority = 0;
    return GuardDeprecatedOperation([&]() {
        return PbBackendCallbackGetExecutionPriorityDeprecated(callback, out_priority);
    });
}

PbStatus PB_CALL pb_callback_set_execution_priority_deprecated(
    PbCallbackHandle callback, int32_t priority)
{
    if (callback.opaque == PB_CALLBACK_INVALID_OPAQUE)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardDeprecatedOperation([&]() {
        return PbBackendCallbackSetExecutionPriorityDeprecated(callback, priority);
    });
}

PbStatus PB_CALL pb_img_entry_deprecated(PbImgHandle image, uint64_t* out_entry)
{
    if (!out_entry || image.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    *out_entry = 0;
    return GuardDeprecatedOperation(
        [&]() { return PbBackendImgEntryDeprecated(image, out_entry); });
}
