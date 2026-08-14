#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbImgHandle image = {0};

    if (pb_img_create_at(
            "pinbridge-synthetic", UINT64_C(0x520000), UINT64_C(0x1000),
            UINT64_C(0x2000), 0, &image) != PB_OK || image.opaque != 52)
        return 1;
    if (pb_img_replay_image_load(image) != PB_OK)
        return 2;

    if (pb_img_create_at(
            0, UINT64_C(0x520000), UINT64_C(0x1000), 0, 0, &image) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_img_create_at("", UINT64_C(0x520000), UINT64_C(0x1000), 0, 0,
            &image) != PB_ERR_INVALID_ARGUMENT ||
        pb_img_create_at("bad", UINT64_C(0x520000), 0, 0, 0, &image) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_img_create_at("bad", UINT64_C(0x520000), 1, 0, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 3;
    image.opaque = 0;
    if (pb_img_replay_image_load(image) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    if (pb_pin_replay_context_change(
            1, (PbConstContextHandle)(uintptr_t)1,
            (PbContextHandle)(uintptr_t)2,
            (PbContextChangeReason)UINT32_C(6), 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
