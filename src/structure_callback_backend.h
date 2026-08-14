#ifndef PINBRIDGE_STRUCTURE_CALLBACK_BACKEND_H
#define PINBRIDGE_STRUCTURE_CALLBACK_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddTraceInstrumentFunction(
    PbTraceInstrumentCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddRtnInstrumentFunction(
    PbRtnInstrumentCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddImgInstrumentFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback);

#endif
