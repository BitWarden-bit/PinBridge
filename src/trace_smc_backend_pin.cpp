#include "pin.H"

#include "trace_smc_backend.h"

#include <cstdlib>

namespace
{

struct SmcState
{
    PbTraceSmcCallback callback;
    void* user_data;
};

VOID OnSmc(ADDRINT trace_start, ADDRINT trace_end, VOID* raw_state)
{
    SmcState* state = static_cast<SmcState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(trace_start), static_cast<uint64_t>(trace_end),
        state->user_data);
}

} // namespace

PbStatus PbBackendTraceAddSmcDetectedFunction(
    PbTraceSmcCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    SmcState* state = static_cast<SmcState*>(std::malloc(sizeof(SmcState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    TRACE_AddSmcDetectedFunction(OnSmc, state);
    return PB_OK;
}
