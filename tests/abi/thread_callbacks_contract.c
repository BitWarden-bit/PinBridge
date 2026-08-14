#include <stdint.h>

#include "pinbridge/pinbridge.h"

_Static_assert(sizeof(PbThreadId) == 4, "PbThreadId must be 32-bit");

static uint32_t g_start_calls;
static uint32_t g_fini_calls;

static void PB_CALL OnThreadStart(
    PbThreadId thread_id, PbContextHandle context, int32_t flags, void* user_data)
{
    if (thread_id == 7u && context != 0 && flags == 9 && user_data == &g_start_calls)
        ++g_start_calls;
}

static void PB_CALL OnThreadFini(
    PbThreadId thread_id, PbConstContextHandle context, int32_t code, void* user_data)
{
    if (thread_id == 7u && context != 0 && code == 37 && user_data == &g_fini_calls)
        ++g_fini_calls;
}

int main(void)
{
    PbCallbackHandle callback = {99};
    if (pb_pin_add_thread_start_function(
            OnThreadStart, &g_start_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_start_calls != 1)
        return 1;
    if (pb_pin_add_thread_fini_function(
            OnThreadFini, &g_fini_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_fini_calls != 1)
        return 2;
    if (pb_pin_add_thread_start_function(0, 0, &callback) !=
            PB_ERR_INVALID_ARGUMENT || callback.opaque != 0 ||
        pb_pin_add_thread_fini_function(OnThreadFini, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 3;
    return 0;
}
