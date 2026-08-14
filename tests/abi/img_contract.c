#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_unload_calls;

static void PB_CALL OnUnload(PbImgHandle image, void* user_data)
{
    if (image.opaque == 73 && user_data == &g_unload_calls)
        ++g_unload_calls;
}

int main(void)
{
    PbImgHandle image = {0};
    PbCallbackHandle callback = {0};
    PbImgType image_type = PB_IMG_TYPE_INVALID;
    uint8_t has_property = 0;
    char name[32];
    uint64_t required = 0;

    if (sizeof(PbImgProperty) != 4 || sizeof(PbImgType) != 4 ||
        PB_IMG_PROPERTY_INVALID != 0 || PB_IMG_PROPERTY_SHSTK_ENABLED != 1 ||
        PB_IMG_PROPERTY_IBT_ENABLED != 2 || PB_IMG_PROPERTY_LAST != 3 ||
        PB_IMG_TYPE_INVALID != 0 || PB_IMG_TYPE_STATIC != 1 ||
        PB_IMG_TYPE_SHARED != 2 || PB_IMG_TYPE_SHAREDLIB != 3 ||
        PB_IMG_TYPE_RELOCATABLE != 4 || PB_IMG_TYPE_DYNAMIC_CODE != 5 ||
        PB_IMG_TYPE_API_CREATED != 6 || PB_IMG_TYPE_LAST != 7)
        return 1;
    if (pb_app_img_head(&image) != PB_OK || image.opaque != 71 ||
        pb_app_img_tail(&image) != PB_OK || image.opaque != 72)
        return 2;
    if (pb_img_add_unload_function(
            OnUnload, &g_unload_calls, &callback) != PB_OK ||
        callback.opaque == PB_CALLBACK_INVALID_OPAQUE || g_unload_calls != 1)
        return 3;
    if (pb_img_find_by_address(UINT64_C(0x1234), &image) != PB_OK ||
        image.opaque != 74 || pb_img_find_by_id(99, &image) != PB_OK ||
        image.opaque != 75 || pb_img_invalid(&image) != PB_OK ||
        image.opaque != 0)
        return 4;
    if (pb_img_open("mock-image.exe", &image) != PB_OK || image.opaque != 76)
        return 5;
    if (pb_img_name(image, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != 15 ||
        pb_img_name(image, name, sizeof(name), &required) != PB_OK ||
        strcmp(name, "mock-image.exe") != 0)
        return 6;
    if (pb_img_type(image, &image_type) != PB_OK ||
        pb_img_has_property(
            image, PB_IMG_PROPERTY_INVALID, &has_property) != PB_OK)
        return 7;
    if (pb_img_close(image) != PB_OK)
        return 8;

    image.opaque = 1;
    if (pb_app_img_head(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_app_img_tail(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_add_unload_function(0, 0, &callback) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_add_unload_function(OnUnload, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_close(image) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_find_by_address(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_find_by_id(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_invalid(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_name(image, name, sizeof(name), &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_name((PbImgHandle){76}, name, sizeof(name), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_img_open(0, &image) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_open("", &image) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_open("mock-image.exe", 0) != PB_ERR_INVALID_ARGUMENT)
        return 9;
    return 0;
}
