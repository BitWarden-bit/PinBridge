#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_calls;

static void PB_CALL OnProbeCall(void* user_data)
{
    if (user_data == &g_calls)
        ++g_calls;
}

int main(void)
{
    uint8_t inserted = 99;
    if (pb_pin_insert_call_probed(
            UINT64_C(0x123456789abcdef0), OnProbeCall, &g_calls, &inserted) != PB_OK ||
        inserted != 1 || g_calls != 1)
        return 1;
    inserted = 99;
    if (pb_pin_insert_call_probed(0, OnProbeCall, 0, &inserted) !=
            PB_ERR_INVALID_ARGUMENT || inserted != 0)
        return 2;
    if (pb_pin_insert_call_probed(1, 0, 0, &inserted) != PB_ERR_INVALID_ARGUMENT ||
        inserted != 0 ||
        pb_pin_insert_call_probed(1, OnProbeCall, 0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 3;
    return 0;
}
