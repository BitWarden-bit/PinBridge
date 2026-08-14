#include "pinbridge/pinbridge.h"

#include "img_backend.h"

namespace
{

template< typename Function > PbStatus GuardImg(Function function)
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

bool IsValid(PbImgHandle image)
{
    return image.opaque > 0;
}

template< typename Function >
PbStatus StoreImage(PbImgHandle* out_image, Function function)
{
    if (!out_image)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardImg([&]() -> PbStatus {
        out_image->opaque = function();
        return PB_OK;
    });
}

} // namespace

PbStatus PB_CALL pb_app_img_head(PbImgHandle* out_image)
{
    return StoreImage(out_image, PbBackendAppImgHead);
}

PbStatus PB_CALL pb_app_img_tail(PbImgHandle* out_image)
{
    return StoreImage(out_image, PbBackendAppImgTail);
}

PbStatus PB_CALL pb_img_add_unload_function(
    PbImgInstrumentCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    out_callback->opaque = PB_CALLBACK_INVALID_OPAQUE;
    return GuardImg([&]() {
        return PbBackendAddImgUnloadFunction(
            callback, user_data, &out_callback->opaque);
    });
}

PbStatus PB_CALL pb_img_close(PbImgHandle image)
{
    if (!IsValid(image))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardImg([&]() { return PbBackendImgClose(image); });
}

PbStatus PB_CALL pb_img_find_by_address(
    uint64_t address, PbImgHandle* out_image)
{
    return StoreImage(out_image, [&]() {
        return PbBackendImgFindByAddress(address);
    });
}

PbStatus PB_CALL pb_img_find_by_id(uint32_t id, PbImgHandle* out_image)
{
    return StoreImage(out_image, [&]() { return PbBackendImgFindById(id); });
}

PbStatus PB_CALL pb_img_invalid(PbImgHandle* out_image)
{
    return StoreImage(out_image, PbBackendImgInvalid);
}

PbStatus PB_CALL pb_img_name(
    PbImgHandle image, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    if (!IsValid(image) || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardImg([&]() {
        return PbBackendImgName(
            image, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_img_open(const char* filename, PbImgHandle* out_image)
{
    if (!filename || filename[0] == '\0' || !out_image)
        return PB_ERR_INVALID_ARGUMENT;
    out_image->opaque = 0;
    return GuardImg([&]() {
        return PbBackendImgOpen(filename, out_image);
    });
}
