#ifndef PINBRIDGE_INS_INSTRUMENTATION_BACKEND_H
#define PINBRIDGE_INS_INSTRUMENTATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendInsInsertCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PbStatus PbBackendInsInsertCallBeforeCtx(
    PbInsHandle ins, PbInsContextAnalysisCallback callback, void* user_data);
PbStatus PbBackendInsInsertIfCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data);
PbStatus PbBackendInsInsertThenCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PbStatus PbBackendInsInsertPredicatedCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PbStatus PbBackendInsInsertIfPredicatedCallBefore(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data);
PbStatus PbBackendInsInsertThenPredicatedCallBefore(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PbStatus PbBackendInsInsertFillBuffer(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset);
PbStatus PbBackendInsInsertFillBufferPredicated(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset);
PbStatus PbBackendInsInsertFillBufferThen(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset);

#endif
