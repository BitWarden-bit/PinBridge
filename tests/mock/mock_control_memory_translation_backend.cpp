#include "control_memory_translation_backend.h"

namespace
{

PbMemoryAddressTransCallback g_callback;

} // namespace

PbStatus PbBackendAddMemoryAddressTransFunction(
    PbMemoryAddressTransCallback callback, void* user_data)
{
    PbMemoryTransInfo info = {};
    info.address = UINT64_C(0x1000);
    info.size = 8u;
    info.instruction_pointer = UINT64_C(0x2000);
    info.thread_id = 7u;
    info.memory_operation = 2u;
    info.is_atomic = 1u;
    if (callback(&info, user_data) != UINT64_C(0x1010))
        return PB_ERR_INTERNAL;
    g_callback = callback;
    return PB_OK;
}

PbStatus PbBackendGetMemoryAddressTransFunction(
    PbMemoryAddressTransCallback* out_callback)
{
    *out_callback = g_callback;
    return PB_OK;
}
