#ifndef PINBRIDGE_TRACE_VERSION_BACKEND_H
#define PINBRIDGE_TRACE_VERSION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendBblSetTargetVersion(PbBblHandle bbl, uint64_t version);
PbStatus PbBackendTraceVersion(PbTraceHandle trace, uint64_t* out_version);
PbStatus PbBackendInsInsertVersionCase(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version);
PbStatus PbBackendInsInsertVersionCaseWithCallOrder(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version,
    PbCallOrder call_order);

#endif
