#include "pin.H"

#include "persistent_callback_state.h"
#include "trace_instrumentation_backend.h"

namespace
{

typedef PbPersistentCallbackState<PbTraceAnalysisCallback> AnalysisState;
typedef PbPersistentCallbackState<PbTracePredicateCallback> PredicateState;

AnalysisState* g_analysis_states = 0;
PredicateState* g_predicate_states = 0;

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

PbStatus PbBackendTraceInsertCallBefore(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    TRACE_InsertCall(
        reinterpret_cast<TRACE>(trace), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendTraceInsertIfCallBefore(
    PbTraceHandle trace, PbTracePredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state = PbInternPersistentCallbackState(
        g_predicate_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    TRACE_InsertIfCall(
        reinterpret_cast<TRACE>(trace), IPOINT_BEFORE, AFUNPTR(OnPredicate),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendTraceInsertThenCallBefore(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    TRACE_InsertThenCall(
        reinterpret_cast<TRACE>(trace), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}
