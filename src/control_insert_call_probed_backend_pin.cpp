#include "pin.H"

#include "control_insert_call_probed_backend.h"

#include <cstdlib>

namespace
{

struct ProbedCallState
{
    PbProbedCallCallback callback;
    void* user_data;
};

VOID OnProbedCall(VOID* raw_state)
{
    ProbedCallState* state = static_cast<ProbedCallState*>(raw_state);
    state->callback(state->user_data);
}

} // namespace

PbStatus PbBackendInsertCallProbed(
    uint64_t address, PbProbedCallCallback callback,
    void* user_data, uint8_t* out_inserted)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ProbedCallState* state =
        static_cast<ProbedCallState*>(std::malloc(sizeof(ProbedCallState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const BOOL inserted = PIN_InsertCallProbed(
        static_cast<ADDRINT>(address), AFUNPTR(OnProbedCall),
        IARG_PTR, state, IARG_END);
    if (!inserted)
        std::free(state);
    *out_inserted = inserted ? 1u : 0u;
    return PB_OK;
}
