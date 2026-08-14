#include <cstring>

#include "replay_backend.h"

namespace
{

bool g_image_created;

} // namespace

PbStatus PbBackendImgCreateAt(
    const char* filename, uint64_t start, uint64_t size, uint64_t load_offset,
    uint8_t main_executable, PbImgHandle* out_image)
{
    if (std::strcmp(filename, "pinbridge-synthetic") != 0 ||
        start != UINT64_C(0x520000) || size != UINT64_C(0x1000) ||
        load_offset != UINT64_C(0x2000) || main_executable != 0)
        return PB_ERR_INTERNAL;
    out_image->opaque = 52;
    g_image_created = true;
    return PB_OK;
}

PbStatus PbBackendImgReplayImageLoad(PbImgHandle image)
{
    return g_image_created && image.opaque == 52 ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendReplayContextChange(
    PbThreadId, PbConstContextHandle, PbContextHandle,
    PbContextChangeReason, int32_t)
{
    return PB_ERR_INTERNAL;
}
