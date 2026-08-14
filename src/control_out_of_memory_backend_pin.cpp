#include "pin.H"

#include "control_out_of_memory_backend.h"

#include <cstdlib>

namespace
{

struct CallbackState
{
    PbOutOfMemoryCallback callback;
    void* user_data;
};

VOID OnOutOfMemory(size_t requested_size, VOID* raw_state)
{
    const CallbackState* state = static_cast<const CallbackState*>(raw_state);
    state->callback(static_cast<uint64_t>(requested_size), state->user_data);
}

} // namespace

PbStatus PbBackendAddOutOfMemoryFunction(
    PbOutOfMemoryCallback callback, void* user_data)
{
    if (!callback)
    {
        PIN_AddOutOfMemoryFunction(0, 0);
        return PB_OK;
    }

    CallbackState* state = static_cast<CallbackState*>(std::malloc(sizeof(CallbackState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;

    // Pin exposes no retirement callback for replaced OOM state. Keeping each
    // immutable state alive avoids racing an in-flight, non-serialized callback.
    PIN_AddOutOfMemoryFunction(OnOutOfMemory, state);
    return PB_OK;
}
