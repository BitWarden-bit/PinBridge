#include "pin.H"

#include "trace_instrumentation_backend.h"

#include <cstdlib>

namespace
{

struct AnalysisState
{
    PbTraceAnalysisCallback callback;
    void* user_data;
};

struct PredicateState
{
    PbTracePredicateCallback callback;
    void* user_data;
};

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
    PbTraceAnalysisCallback callback, void* user_data, AnalysisState** out_state)
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

PbStatus PbBackendTraceInsertCallBefore(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
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
    PredicateState* state =
        static_cast<PredicateState*>(std::malloc(sizeof(PredicateState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
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
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    TRACE_InsertThenCall(
        reinterpret_cast<TRACE>(trace), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}
