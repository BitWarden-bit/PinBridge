#include "img_backend.h"

#include <cstring>

namespace
{

bool g_image_open;

} // namespace

int32_t PbBackendAppImgHead(void)
{
    return 71;
}

int32_t PbBackendAppImgTail(void)
{
    return 72;
}

PbStatus PbBackendAddImgUnloadFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x2055);
    const PbImgHandle image = {73};
    callback(image, user_data);
    return PB_OK;
}

PbStatus PbBackendImgClose(PbImgHandle image)
{
    if (!g_image_open || image.opaque != 76)
        return PB_ERR_INVALID_ARGUMENT;
    g_image_open = false;
    return PB_OK;
}

int32_t PbBackendImgFindByAddress(uint64_t address)
{
    return address == UINT64_C(0x1234) ? 74 : 0;
}

int32_t PbBackendImgFindById(uint32_t id)
{
    return id == 99 ? 75 : 0;
}

int32_t PbBackendImgInvalid(void)
{
    return 0;
}

PbStatus PbBackendImgName(
    PbImgHandle image, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    static const char kName[] = "mock-image.exe";
    if (!g_image_open || image.opaque != 76)
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = sizeof(kName);
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, kName, sizeof(kName));
    return PB_OK;
}

PbStatus PbBackendImgOpen(const char* filename, PbImgHandle* out_image)
{
    if (g_image_open)
        return PB_ERR_INVALID_STATE;
    if (std::strcmp(filename, "mock-image.exe") != 0)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    g_image_open = true;
    out_image->opaque = 76;
    return PB_OK;
}
