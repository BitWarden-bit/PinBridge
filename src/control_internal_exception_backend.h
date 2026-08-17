#ifndef PINBRIDGE_CONTROL_INTERNAL_EXCEPTION_BACKEND_H
#define PINBRIDGE_CONTROL_INTERNAL_EXCEPTION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddInternalExceptionHandler(
    PbInternalExceptionCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendEnableSingleStepPassthrough(uint64_t* out_callback);
PbStatus PbBackendTryStart(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    uint64_t* out_scope);
PbStatus PbBackendTryEnd(PbThreadId thread_id, uint64_t scope);

#endif
