#include "pin.H"

#include "buffer_backend.h"

#include <cstdlib>

namespace
{

static_assert(sizeof(BUFFER_ID) == sizeof(PbBufferId),
              "Pin 3.31 BUFFER_ID width changed");
static_assert(BUFFER_ID_INVALID == 0, "Pin 3.31 BUFFER_ID_INVALID changed");

struct BufferState
{
    PbTraceBufferCallback callback;
    void* user_data;
};

VOID* OnBuffer(
    BUFFER_ID id, THREADID thread_id, const CONTEXT* context,
    VOID* buffer, UINT64 num_elements, VOID* raw_state)
{
    BufferState* state = static_cast<BufferState*>(raw_state);
    return state->callback(
        static_cast<PbBufferId>(id), static_cast<PbThreadId>(thread_id),
        reinterpret_cast<PbConstContextHandle>(context), buffer,
        static_cast<uint64_t>(num_elements), state->user_data);
}

} // namespace

PbStatus PbBackendDefineTraceBuffer(
    uint64_t record_size, uint32_t num_pages,
    PbTraceBufferCallback callback, void* user_data, PbBufferId* out_id)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    BufferState* state = static_cast<BufferState*>(std::malloc(sizeof(BufferState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const BUFFER_ID id = PIN_DefineTraceBuffer(
        static_cast<size_t>(record_size), static_cast<UINT32>(num_pages),
        OnBuffer, state);
    if (id == BUFFER_ID_INVALID)
    {
        std::free(state);
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    }
    *out_id = static_cast<PbBufferId>(id);
    return PB_OK;
}

PbStatus PbBackendAllocateBuffer(PbBufferId id, void** out_buffer)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_buffer = PIN_AllocateBuffer(static_cast<BUFFER_ID>(id));
    return *out_buffer ? PB_OK : PB_ERR_OUT_OF_MEMORY;
}

PbStatus PbBackendDeallocateBuffer(PbBufferId id, void* buffer)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PIN_DeallocateBuffer(static_cast<BUFFER_ID>(id), buffer);
    return PB_OK;
}

PbStatus PbBackendGetBufferPointer(
    PbContextHandle context, PbBufferId id, void** out_buffer)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_buffer = PIN_GetBufferPointer(
        reinterpret_cast<CONTEXT*>(context), static_cast<BUFFER_ID>(id));
    return *out_buffer ? PB_OK : PB_ERR_INTERNAL;
}
