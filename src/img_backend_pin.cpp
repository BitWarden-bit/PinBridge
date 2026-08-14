#include "pin.H"

#include "img_backend.h"

#include <cstdlib>
#include <cstring>
#include <string>

namespace
{

static_assert(PB_IMG_PROPERTY_INVALID == IMG_PROPERTY_INVALID,
              "IMG_PROPERTY_INVALID value drift");
static_assert(PB_IMG_PROPERTY_SHSTK_ENABLED == IMG_PROPERTY_SHSTK_ENABLED,
              "IMG_PROPERTY_SHSTK_ENABLED value drift");
static_assert(PB_IMG_PROPERTY_IBT_ENABLED == IMG_PROPERTY_IBT_ENABLED,
              "IMG_PROPERTY_IBT_ENABLED value drift");
static_assert(PB_IMG_PROPERTY_LAST == IMG_PROPERTY_LAST,
              "IMG_PROPERTY_LAST value drift");
static_assert(PB_IMG_TYPE_INVALID == IMG_TYPE_INVALID,
              "IMG_TYPE_INVALID value drift");
static_assert(PB_IMG_TYPE_STATIC == IMG_TYPE_STATIC, "IMG_TYPE_STATIC value drift");
static_assert(PB_IMG_TYPE_SHARED == IMG_TYPE_SHARED, "IMG_TYPE_SHARED value drift");
static_assert(PB_IMG_TYPE_SHAREDLIB == IMG_TYPE_SHAREDLIB,
              "IMG_TYPE_SHAREDLIB value drift");
static_assert(PB_IMG_TYPE_RELOCATABLE == IMG_TYPE_RELOCATABLE,
              "IMG_TYPE_RELOCATABLE value drift");
static_assert(PB_IMG_TYPE_DYNAMIC_CODE == IMG_TYPE_DYNAMIC_CODE,
              "IMG_TYPE_DYNAMIC_CODE value drift");
static_assert(PB_IMG_TYPE_API_CREATED == IMG_TYPE_API_CREATED,
              "IMG_TYPE_API_CREATED value drift");
static_assert(PB_IMG_TYPE_LAST == IMG_TYPE_LAST, "IMG_TYPE_LAST value drift");

struct ImgCallbackState
{
    PbImgInstrumentCallback callback;
    void* user_data;
};

int32_t g_open_image;

IMG ToImg(PbImgHandle image)
{
    IMG result;
    result.q_set(image.opaque);
    return result;
}

VOID OnImgUnload(IMG image, VOID* raw_state)
{
    ImgCallbackState* state = static_cast<ImgCallbackState*>(raw_state);
    const PbImgHandle bridge_image = {image.q()};
    state->callback(bridge_image, state->user_data);
}

} // namespace

int32_t PbBackendAppImgHead(void)
{
    return APP_ImgHead().q();
}

int32_t PbBackendAppImgTail(void)
{
    return APP_ImgTail().q();
}

PbStatus PbBackendAddImgUnloadFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    ImgCallbackState* state =
        static_cast<ImgCallbackState*>(std::malloc(sizeof(ImgCallbackState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK registration = IMG_AddUnloadFunction(OnImgUnload, state);
    if (registration == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(
        reinterpret_cast<uintptr_t>(registration));
    return PB_OK;
}

PbStatus PbBackendImgClose(PbImgHandle image)
{
    if (g_open_image == 0 || image.opaque != g_open_image)
        return PB_ERR_INVALID_ARGUMENT;
    IMG_Close(ToImg(image));
    g_open_image = 0;
    return PB_OK;
}

int32_t PbBackendImgFindByAddress(uint64_t address)
{
    return IMG_FindByAddress(static_cast<ADDRINT>(address)).q();
}

int32_t PbBackendImgFindById(uint32_t id)
{
    return IMG_FindImgById(static_cast<UINT32>(id)).q();
}

int32_t PbBackendImgInvalid(void)
{
    return IMG_Invalid().q();
}

PbStatus PbBackendImgName(
    PbImgHandle image, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    const std::string& name = IMG_Name(ToImg(image));
    *required_size = static_cast<uint64_t>(name.size()) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, name.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}

PbStatus PbBackendImgOpen(const char* filename, PbImgHandle* out_image)
{
    if (g_open_image != 0)
        return PB_ERR_INVALID_STATE;
    const IMG image = IMG_Open(std::string(filename));
    if (!IMG_Valid(image))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    g_open_image = image.q();
    out_image->opaque = g_open_image;
    return PB_OK;
}
