#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_jit_calls;
static uint32_t g_probe_calls;

static void PB_CALL OnJitAttach(void* user_data)
{
    if (user_data == &g_jit_calls)
        ++g_jit_calls;
}

static void PB_CALL OnProbeAttach(void* user_data)
{
    if (user_data == &g_probe_calls)
        ++g_probe_calls;
}

int main(void)
{
    PbAttachStatus status = PB_ATTACH_FAILED_DETACH;

    if (pb_pin_attach(0, 0, &status) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_attach(OnJitAttach, &g_jit_calls, 0) != PB_ERR_INVALID_ARGUMENT)
        return 1;
    if (pb_pin_attach(OnJitAttach, &g_jit_calls, &status) != PB_OK ||
        status != PB_ATTACH_INITIATED || g_jit_calls != 1)
        return 2;

    if (pb_pin_attach_probed(0, 0, &status) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_attach_probed(OnProbeAttach, &g_probe_calls, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 3;
    if (pb_pin_attach_probed(OnProbeAttach, &g_probe_calls, &status) != PB_OK ||
        status != PB_ATTACH_INITIATED || g_probe_calls != 1)
        return 4;
    return 0;
}
