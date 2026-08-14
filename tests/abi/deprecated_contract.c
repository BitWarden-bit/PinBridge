#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbCallbackHandle callback = {UINT64_C(0x5101)};
    PbImgHandle image = {51};
    int32_t priority = 0;
    uint64_t entry = 0;

    if (pb_callback_get_execution_priority_deprecated(callback, &priority) != PB_OK ||
        priority != 200)
        return 1;
    if (pb_callback_set_execution_priority_deprecated(callback, 117) != PB_OK)
        return 2;
    if (pb_callback_get_execution_priority_deprecated(callback, &priority) != PB_OK ||
        priority != 117)
        return 3;
    if (pb_img_entry_deprecated(image, &entry) != PB_OK ||
        entry != UINT64_C(0x405100))
        return 4;

    callback.opaque = PB_CALLBACK_INVALID_OPAQUE;
    if (pb_callback_get_execution_priority_deprecated(callback, &priority) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_callback_set_execution_priority_deprecated(callback, 200) !=
            PB_ERR_INVALID_ARGUMENT)
        return 5;
    callback.opaque = UINT64_C(0x5101);
    if (pb_callback_get_execution_priority_deprecated(callback, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 6;
    image.opaque = 0;
    if (pb_img_entry_deprecated(image, &entry) != PB_ERR_INVALID_ARGUMENT)
        return 7;
    image.opaque = 51;
    if (pb_img_entry_deprecated(image, 0) != PB_ERR_INVALID_ARGUMENT)
        return 8;
    return 0;
}
