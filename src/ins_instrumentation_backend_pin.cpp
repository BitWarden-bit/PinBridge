#include "pin.H"

#include "ins_instrumentation_backend.h"
#include "persistent_callback_state.h"

namespace
{

typedef PbPersistentCallbackState<PbInsAnalysisCallback> AnalysisState;
typedef PbPersistentCallbackState<PbInsContextAnalysisCallback> ContextAnalysisState;
typedef PbPersistentCallbackState<PbInsPredicateCallback> PredicateState;

AnalysisState* g_analysis_states = 0;
ContextAnalysisState* g_context_analysis_states = 0;
PredicateState* g_predicate_states = 0;

INS ToIns(PbInsHandle handle)
{
    INS ins;
    ins.q_set(handle.opaque);
    return ins;
}

IPOINT ToIpoint(PbIpoint ipoint)
{
    return static_cast<IPOINT>(ipoint);
}

VOID OnAnalysis(VOID* raw_state)
{
    AnalysisState* state = static_cast<AnalysisState*>(raw_state);
    state->callback(state->user_data);
}

VOID OnContextAnalysis(CONTEXT* context, VOID* raw_state)
{
    ContextAnalysisState* state = static_cast<ContextAnalysisState*>(raw_state);
    state->callback(reinterpret_cast<PbContextHandle>(context), state->user_data);
}

ADDRINT OnPredicate(VOID* raw_state)
{
    PredicateState* state = static_cast<PredicateState*>(raw_state);
    return static_cast<ADDRINT>(state->callback(state->user_data));
}

} // namespace

PbStatus PbBackendInsInsertCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                   IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertCallBeforeCtx(
    PbInsHandle ins, PbInsContextAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ContextAnalysisState* state = PbInternPersistentCallbackState(
        g_context_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnContextAnalysis),
                   IARG_CONTEXT, IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state = PbInternPersistentCallbackState(
        g_predicate_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertIfCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnPredicate),
                     IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertThenCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertThenCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                       IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertPredicatedCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertPredicatedCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                             IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfPredicatedCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state = PbInternPersistentCallbackState(
        g_predicate_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertIfPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnPredicate),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertThenPredicatedCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = PbInternPersistentCallbackState(
        g_analysis_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertThenPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBuffer(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    INS_InsertFillBuffer(
        ToIns(ins), ToIpoint(ipoint), static_cast<BUFFER_ID>(id),
        IARG_INST_PTR, static_cast<UINT32>(offset), IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBufferPredicated(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    INS_InsertFillBufferPredicated(
        ToIns(ins), ToIpoint(ipoint), static_cast<BUFFER_ID>(id),
        IARG_INST_PTR, static_cast<UINT32>(offset), IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertFillBufferThen(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    INS_InsertFillBufferThen(
        ToIns(ins), ToIpoint(ipoint), static_cast<BUFFER_ID>(id),
        IARG_INST_PTR, static_cast<UINT32>(offset), IARG_END);
    return PB_OK;
}
