#ifndef PINBRIDGE_CONTROL_CALLBACK_BACKEND_H
#define PINBRIDGE_CONTROL_CALLBACK_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddApplicationStartFunction(
    PbApplicationStartCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddPrepareForFiniFunction(
    PbPrepareForFiniCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddFiniFunction(
    PbFiniCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddThreadStartFunction(
    PbThreadStartCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddThreadFiniFunction(
    PbThreadFiniCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddContextChangeFunction(
    PbContextChangeCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddXedDecodeCallbackFunction(
    PbXedDecodeCallback callback, void* user_data);

#endif
