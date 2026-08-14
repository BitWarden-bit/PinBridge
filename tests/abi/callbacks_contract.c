#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbCallbackHandle callback = {UINT64_C(0x4801)};
    PbCallOrder order = 0;

    if (PB_CALLBACK_INVALID_OPAQUE != UINT64_C(0))
        return 1;
    if (pb_callback_get_execution_order(callback, &order) != PB_OK ||
        order != PB_CALL_ORDER_DEFAULT)
        return 2;
    if (pb_callback_set_execution_order(
            callback, PB_CALL_ORDER_FIRST + 7) != PB_OK)
        return 3;
    if (pb_callback_get_execution_order(callback, &order) != PB_OK ||
        order != PB_CALL_ORDER_FIRST + 7)
        return 4;

    callback.opaque = PB_CALLBACK_INVALID_OPAQUE;
    if (pb_callback_get_execution_order(callback, &order) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_callback_set_execution_order(callback, PB_CALL_ORDER_DEFAULT) !=
            PB_ERR_INVALID_ARGUMENT)
        return 5;
    callback.opaque = UINT64_C(0x4801);
    if (pb_callback_get_execution_order(callback, 0) != PB_ERR_INVALID_ARGUMENT)
        return 6;
    return 0;
}
