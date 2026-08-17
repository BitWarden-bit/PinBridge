#include "pin.H"

#include "bbl_instrumentation_backend.h"
#include "persistent_callback_state.h"

namespace
{

typedef PbPersistentCallbackState<PbBblAnalysisCallback> AnalysisState;
typedef PbPersistentCallbackState<PbBblPredicateCallback> PredicateState;

AnalysisState* g_analysis_states = 0;
PredicateState* g_predicate_states = 0;

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

} // namespace

PbStatus PbBackendBblInsertCallBefore(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
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
    PredicateState* state = PbInternPersistentCallbackState(
        g_predicate_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
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
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    BBL_InsertThenCall(
        ToBbl(bbl), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}
