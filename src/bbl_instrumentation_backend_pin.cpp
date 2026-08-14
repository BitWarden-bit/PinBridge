#include "pin.H"

#include "bbl_instrumentation_backend.h"

#include <cstdlib>

namespace
{

struct AnalysisState
{
    PbBblAnalysisCallback callback;
    void* user_data;
};

struct PredicateState
{
    PbBblPredicateCallback callback;
    void* user_data;
};

BBL ToBbl(PbBblHandle handle)
{
    BBL bbl;
    bbl.q_set(handle.opaque);
    return bbl;
}

VOID OnAnalysis(VOID* raw_state)
{
    AnalysisState* state = static_cast<AnalysisState*>(raw_state);
    state->callback(state->user_data);
}

ADDRINT OnPredicate(VOID* raw_state)
{
    PredicateState* state = static_cast<PredicateState*>(raw_state);
    return static_cast<ADDRINT>(state->callback(state->user_data));
}

PbStatus NewAnalysisState(
    PbBblAnalysisCallback callback, void* user_data, AnalysisState** out_state)
{
    AnalysisState* state =
        static_cast<AnalysisState*>(std::malloc(sizeof(AnalysisState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    *out_state = state;
    return PB_OK;
}

} // namespace

PbStatus PbBackendBblInsertCallBefore(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    BBL_InsertCall(
        ToBbl(bbl), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendBblInsertIfCallBefore(
    PbBblHandle bbl, PbBblPredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state =
        static_cast<PredicateState*>(std::malloc(sizeof(PredicateState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    BBL_InsertIfCall(
        ToBbl(bbl), IPOINT_BEFORE, AFUNPTR(OnPredicate),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendBblInsertThenCallBefore(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    BBL_InsertThenCall(
        ToBbl(bbl), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}
