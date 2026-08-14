#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_calls;
static uint64_t g_requested_size;

static void PB_CALL OnOutOfMemory(uint64_t requested_size, void* user_data)
{
    if (user_data == &g_calls)
    {
        ++g_calls;
        g_requested_size = requested_size;
    }
}

int main(void)
{
    if (pb_pin_add_out_of_memory_function(0, 0) != PB_OK || g_calls != 0)
        return 1;
    if (pb_pin_add_out_of_memory_function(OnOutOfMemory, &g_calls) != PB_OK)
        return 2;
    if (g_calls != 1 || g_requested_size != UINT64_C(0x123456789ABCDEF0))
        return 3;
    if (pb_pin_add_out_of_memory_function(0, &g_calls) != PB_OK || g_calls != 1)
        return 4;
    return 0;
}
