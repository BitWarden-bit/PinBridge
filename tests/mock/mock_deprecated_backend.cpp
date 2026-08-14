#include "deprecated_backend.h"

namespace
{

int32_t g_priority = 200;

bool IsExpectedCallback(PbCallbackHandle callback)
{
    return callback.opaque == UINT64_C(0x5101);
}

} // namespace

PbStatus PbBackendCallbackGetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t* out_priority)
{
    if (!IsExpectedCallback(callback))
        return PB_ERR_INTERNAL;
    *out_priority = g_priority;
    return PB_OK;
}

PbStatus PbBackendCallbackSetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t priority)
{
    if (!IsExpectedCallback(callback))
        return PB_ERR_INTERNAL;
    g_priority = priority;
    return PB_OK;
}

PbStatus PbBackendImgEntryDeprecated(PbImgHandle image, uint64_t* out_entry)
{
    if (image.opaque != 51)
        return PB_ERR_INTERNAL;
    *out_entry = UINT64_C(0x405100);
    return PB_OK;
}
