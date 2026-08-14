#include "pin.H"

#include "ins_instrumentation_backend.h"

#include <cstdlib>

namespace
{

struct AnalysisState
{
    PbInsAnalysisCallback callback;
    void* user_data;
};

struct ContextAnalysisState
{
    PbInsContextAnalysisCallback callback;
    void* user_data;
};

struct PredicateState
{
    PbInsPredicateCallback callback;
    void* user_data;
};

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

PbStatus NewAnalysisState(
    PbInsAnalysisCallback callback, void* user_data, AnalysisState** out_state)
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

PbStatus NewPredicateState(
    PbInsPredicateCallback callback, void* user_data,
    PredicateState** out_state)
{
    PredicateState* state =
        static_cast<PredicateState*>(std::malloc(sizeof(PredicateState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    *out_state = state;
    return PB_OK;
}

} // namespace

PbStatus PbBackendInsInsertCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    INS_InsertCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                   IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertCallBeforeCtx(
    PbInsHandle ins, PbInsContextAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ContextAnalysisState* state =
        static_cast<ContextAnalysisState*>(std::malloc(sizeof(ContextAnalysisState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    INS_InsertCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnContextAnalysis),
                   IARG_CONTEXT, IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state = 0;
    const PbStatus status = NewPredicateState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    INS_InsertIfCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnPredicate),
                     IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertThenCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    INS_InsertThenCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                       IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertPredicatedCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
    INS_InsertPredicatedCall(ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnAnalysis),
                             IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertIfPredicatedCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    PredicateState* state = 0;
    const PbStatus status = NewPredicateState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
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
    AnalysisState* state = 0;
    const PbStatus status = NewAnalysisState(callback, user_data, &state);
    if (status != PB_OK)
        return status;
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
