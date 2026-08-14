#include "pin.H"

#include "deprecated_backend.h"

namespace
{

static_assert(sizeof(PIN_CALLBACK) <= sizeof(uint64_t),
              "PIN_CALLBACK no longer fits PbCallbackHandle");
static_assert(sizeof(IMG) == sizeof(int32_t), "Pin 3.31 IMG layout changed");

PIN_CALLBACK ToPinCallback(PbCallbackHandle callback)
{
    return reinterpret_cast<PIN_CALLBACK>(static_cast<uintptr_t>(callback.opaque));
}

IMG ToPinImg(PbImgHandle image)
{
    IMG result;
    result.q_set(image.opaque);
    return result;
}

} // namespace

PbStatus PbBackendCallbackGetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t* out_priority)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_priority = static_cast<int32_t>(
        CALLBACK_GetExecutionPriority(ToPinCallback(callback)));
    return PB_OK;
}

PbStatus PbBackendCallbackSetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t priority)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    CALLBACK_SetExecutionPriority(ToPinCallback(callback), priority);
    return PB_OK;
}

PbStatus PbBackendImgEntryDeprecated(PbImgHandle image, uint64_t* out_entry)
{
    *out_entry = static_cast<uint64_t>(IMG_Entry(ToPinImg(image)));
    return PB_OK;
}
