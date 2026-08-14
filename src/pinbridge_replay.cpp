#include "pinbridge/pinbridge.h"

#include "replay_backend.h"

namespace
{

template< typename Function > PbStatus GuardReplayOperation(Function function)
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

PbStatus PB_CALL pb_img_create_at(
    const char* filename, uint64_t start, uint64_t size, uint64_t load_offset,
    uint8_t main_executable, PbImgHandle* out_image)
{
    if (!filename || filename[0] == '\0' || size == 0 || !out_image)
        return PB_ERR_INVALID_ARGUMENT;
    out_image->opaque = 0;
    return GuardReplayOperation([&]() {
        return PbBackendImgCreateAt(
            filename, start, size, load_offset, main_executable, out_image);
    });
}

PbStatus PB_CALL pb_img_replay_image_load(PbImgHandle image)
{
    if (image.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardReplayOperation(
        [&]() { return PbBackendImgReplayImageLoad(image); });
}

PbStatus PB_CALL pb_pin_replay_context_change(
    PbThreadId thread_id, PbConstContextHandle from, PbContextHandle to,
    PbContextChangeReason reason, int32_t info)
{
    if (!from || reason > PB_CONTEXT_CHANGE_REASON_CALLBACK)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return PbBackendReplayContextChange(thread_id, from, to, reason, info);
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return PbBackendReplayContextChange(thread_id, from, to, reason, info);
#endif
}
