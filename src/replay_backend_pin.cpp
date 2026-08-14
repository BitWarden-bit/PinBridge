#include "pin.H"

#include "replay_backend.h"

namespace
{

static_assert(sizeof(IMG) == sizeof(int32_t), "Pin 3.31 IMG layout changed");
static_assert(sizeof(USIZE) == sizeof(uint64_t), "Pin 3.31 USIZE width changed");

IMG ToPinImg(PbImgHandle image)
{
    IMG result;
    result.q_set(image.opaque);
    return result;
}

} // namespace

PbStatus PbBackendImgCreateAt(
    const char* filename, uint64_t start, uint64_t size, uint64_t load_offset,
    uint8_t main_executable, PbImgHandle* out_image)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const IMG image = IMG_CreateAt(
        filename, static_cast<ADDRINT>(start), static_cast<USIZE>(size),
        static_cast<ADDRINT>(load_offset), main_executable != 0);
    out_image->opaque = image.q();
    return IMG_Valid(image) ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendImgReplayImageLoad(PbImgHandle image)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    IMG_ReplayImageLoad(ToPinImg(image));
    return PB_OK;
}

PbStatus PbBackendReplayContextChange(
    PbThreadId thread_id, PbConstContextHandle from, PbContextHandle to,
    PbContextChangeReason reason, int32_t info)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PIN_ReplayContextChange(
        static_cast<THREADID>(thread_id),
        reinterpret_cast<const CONTEXT*>(from), reinterpret_cast<CONTEXT*>(to),
        static_cast<CONTEXT_CHANGE_REASON>(reason), static_cast<INT32>(info));
}
