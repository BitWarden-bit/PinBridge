#ifndef PINBRIDGE_CONTROL_CALL_APPLICATION_BACKEND_H
#define PINBRIDGE_CONTROL_CALL_APPLICATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendCallApplicationVoid0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address);
PbStatus PbBackendCallApplicationU640(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t* out_result);
PbStatus PbBackendCallApplicationU641(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t* out_result);
PbStatus PbBackendCallApplicationU642(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t argument1,
    uint64_t* out_result);
PbStatus PbBackendCallApplicationPtrUsize(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t size, void** out_result);

#endif
