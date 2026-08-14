#include "control_out_of_memory_backend.h"

namespace
{

PbOutOfMemoryCallback g_callback;
void* g_user_data;

} // namespace

PbStatus PbBackendAddOutOfMemoryFunction(
    PbOutOfMemoryCallback callback, void* user_data)
{
    g_callback = callback;
    g_user_data = callback ? user_data : 0;
    if (g_callback)
        g_callback(UINT64_C(0x123456789ABCDEF0), g_user_data);
    return PB_OK;
}
