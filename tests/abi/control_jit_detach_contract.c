#include "pinbridge/pinbridge.h"

static uint32_t g_calls;

static void PB_CALL OnDetach(void* user_data)
{
    if (user_data == &g_calls)
        ++g_calls;
}

int main(void)
{
    PbCallbackHandle callback = {0};

    if (pb_pin_add_detach_function(0, 0, &callback) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_add_detach_function(OnDetach, &g_calls, 0) != PB_ERR_INVALID_ARGUMENT)
        return 1;
    if (pb_pin_add_detach_function(OnDetach, &g_calls, &callback) != PB_OK ||
        callback.opaque == 0)
        return 2;
    if (g_calls != 0)
        return 3;
    if (pb_pin_detach() != PB_OK || g_calls != 1)
        return 4;
    return 0;
}
