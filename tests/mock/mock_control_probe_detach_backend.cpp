#include "control_probe_detach_backend.h"

namespace
{

PbDetachProbedCallback g_callback;
void* g_user_data;

} // namespace

PbStatus PbBackendAddDetachFunctionProbed(
    PbDetachProbedCallback callback, void* user_data, uint64_t* out_callback)
{
    g_callback = callback;
    g_user_data = user_data;
    *out_callback = UINT64_C(0x3500);
    return PB_OK;
}

PbStatus PbBackendDetachProbed(void)
{
    if (!g_callback)
        return PB_ERR_INVALID_STATE;
    PbDetachProbedCallback callback = g_callback;
    void* user_data = g_user_data;
    g_callback = 0;
    g_user_data = 0;
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendDetach(void)
{
    return PB_OK;
}
