#include "pinbridge/pinbridge.h"

#include "buffer_backend.h"

#include <cstddef>
#include <limits>

namespace
{

template< typename Function > PbStatus GuardBufferOperation(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_define_trace_buffer(
    uint64_t record_size, uint32_t num_pages,
    PbTraceBufferCallback callback, void* user_data, PbBufferId* out_id)
{
    if (record_size == 0 || num_pages == 0 || !callback || !out_id)
        return PB_ERR_INVALID_ARGUMENT;
    if (record_size > static_cast<uint64_t>(std::numeric_limits<size_t>::max()))
        return PB_ERR_UNSUPPORTED;
    *out_id = PB_BUFFER_ID_INVALID;
    return GuardBufferOperation([&]() {
        return PbBackendDefineTraceBuffer(
            record_size, num_pages, callback, user_data, out_id);
    });
}

PbStatus PB_CALL pb_pin_allocate_buffer(PbBufferId id, void** out_buffer)
{
    if (id == PB_BUFFER_ID_INVALID || !out_buffer)
        return PB_ERR_INVALID_ARGUMENT;
    *out_buffer = 0;
    return GuardBufferOperation(
        [&]() { return PbBackendAllocateBuffer(id, out_buffer); });
}

PbStatus PB_CALL pb_pin_deallocate_buffer(PbBufferId id, void* buffer)
{
    if (id == PB_BUFFER_ID_INVALID || !buffer)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardBufferOperation(
        [&]() { return PbBackendDeallocateBuffer(id, buffer); });
}

PbStatus PB_CALL pb_pin_get_buffer_pointer(
    PbContextHandle context, PbBufferId id, void** out_buffer)
{
    if (!context || id == PB_BUFFER_ID_INVALID || !out_buffer)
        return PB_ERR_INVALID_ARGUMENT;
    *out_buffer = 0;
    return GuardBufferOperation(
        [&]() { return PbBackendGetBufferPointer(context, id, out_buffer); });
}
