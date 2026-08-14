#ifndef PINBRIDGE_TRACE_INSTRUMENTATION_BACKEND_H
#define PINBRIDGE_TRACE_INSTRUMENTATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendTraceInsertCallBefore(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data);
PbStatus PbBackendTraceInsertIfCallBefore(
    PbTraceHandle trace, PbTracePredicateCallback callback, void* user_data);
PbStatus PbBackendTraceInsertThenCallBefore(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data);

#endif
