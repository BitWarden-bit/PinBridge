#ifndef PINBRIDGE_BBL_INSTRUMENTATION_BACKEND_H
#define PINBRIDGE_BBL_INSTRUMENTATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendBblInsertCallBefore(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data);
PbStatus PbBackendBblInsertIfCallBefore(
    PbBblHandle bbl, PbBblPredicateCallback callback, void* user_data);
PbStatus PbBackendBblInsertThenCallBefore(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data);

#endif
