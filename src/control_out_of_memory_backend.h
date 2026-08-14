#ifndef PINBRIDGE_CONTROL_OUT_OF_MEMORY_BACKEND_H
#define PINBRIDGE_CONTROL_OUT_OF_MEMORY_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddOutOfMemoryFunction(
    PbOutOfMemoryCallback callback, void* user_data);

#endif
