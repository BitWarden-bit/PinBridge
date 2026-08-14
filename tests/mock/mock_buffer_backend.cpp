#include "buffer_backend.h"

namespace
{

uint64_t g_storage[8];

} // namespace

PbStatus PbBackendDefineTraceBuffer(
    uint64_t record_size, uint32_t num_pages,
    PbTraceBufferCallback callback, void* user_data, PbBufferId* out_id)
{
    if (record_size != 16 || num_pages != 2)
        return PB_ERR_INTERNAL;
    *out_id = 7;
    callback(
        7, 9, reinterpret_cast<PbConstContextHandle>(static_cast<uintptr_t>(1)),
        g_storage, 11, user_data);
    return PB_OK;
}

PbStatus PbBackendAllocateBuffer(PbBufferId id, void** out_buffer)
{
    if (id != 7)
        return PB_ERR_INTERNAL;
    *out_buffer = g_storage;
    return PB_OK;
}

PbStatus PbBackendDeallocateBuffer(PbBufferId id, void* buffer)
{
    return id == 7 && buffer == g_storage ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendGetBufferPointer(
    PbContextHandle, PbBufferId id, void** out_buffer)
{
    if (id != 7)
        return PB_ERR_INTERNAL;
    *out_buffer = g_storage;
    return PB_OK;
}
