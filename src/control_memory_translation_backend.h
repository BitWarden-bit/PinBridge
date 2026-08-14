#ifndef PINBRIDGE_CONTROL_MEMORY_TRANSLATION_BACKEND_H
#define PINBRIDGE_CONTROL_MEMORY_TRANSLATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddMemoryAddressTransFunction(
    PbMemoryAddressTransCallback callback, void* user_data);
PbStatus PbBackendGetMemoryAddressTransFunction(
    PbMemoryAddressTransCallback* out_callback);

#endif
