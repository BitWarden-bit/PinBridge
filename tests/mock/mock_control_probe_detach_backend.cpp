#include "control_probe_detach_backend.h"

namespace
{

PbDetachCallback g_jit_callback;
void* g_jit_user_data;
PbDetachProbedCallback g_probed_callback;
void* g_probed_user_data;

} // namespace

PbStatus PbBackendAddDetachFunction(
    PbDetachCallback callback, void* user_data, uint64_t* out_callback)
{
    g_jit_callback = callback;
    g_jit_user_data = user_data;
    *out_callback = UINT64_C(0x3400);
    return PB_OK;
}

PbStatus PbBackendAddDetachFunctionProbed(
    PbDetachProbedCallback callback, void* user_data, uint64_t* out_callback)
{
    g_probed_callback = callback;
    g_probed_user_data = user_data;
    *out_callback = UINT64_C(0x3500);
    return PB_OK;
}

PbStatus PbBackendDetachProbed(void)
{
    if (!g_probed_callback)
        return PB_ERR_INVALID_STATE;
    PbDetachProbedCallback callback = g_probed_callback;
    void* user_data = g_probed_user_data;
    g_probed_callback = 0;
    g_probed_user_data = 0;
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendDetach(void)
{
    if (!g_jit_callback)
        return PB_ERR_INVALID_STATE;
    PbDetachCallback callback = g_jit_callback;
    void* user_data = g_jit_user_data;
    g_jit_callback = 0;
    g_jit_user_data = 0;
    callback(user_data);
    return PB_OK;
}
