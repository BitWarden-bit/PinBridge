#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_calls;
static uint64_t g_start;
static uint64_t g_end;

static void PB_CALL OnSmc(
    uint64_t trace_start, uint64_t trace_end, void* user_data)
{
    if (user_data == &g_calls)
    {
        ++g_calls;
        g_start = trace_start;
        g_end = trace_end;
    }
}

int main(void)
{
    if (pb_trace_add_smc_detected_function(OnSmc, &g_calls) != PB_OK ||
        g_calls != 1 || g_start != UINT64_C(0x1000) ||
        g_end != UINT64_C(0x1010))
        return 1;
    if (pb_trace_add_smc_detected_function(0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 2;
    return 0;
}
