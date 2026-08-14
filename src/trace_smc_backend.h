#ifndef PINBRIDGE_TRACE_SMC_BACKEND_H
#define PINBRIDGE_TRACE_SMC_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendTraceAddSmcDetectedFunction(
    PbTraceSmcCallback callback, void* user_data);

#endif
