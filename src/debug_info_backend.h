#ifndef PINBRIDGE_DEBUG_INFO_BACKEND_H
#define PINBRIDGE_DEBUG_INFO_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendGetSourceLocation(
    uint64_t address, int32_t* column, int32_t* line,
    char* file_name, uint64_t capacity, uint64_t* required_size);

#endif
