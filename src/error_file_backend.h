#ifndef PINBRIDGE_ERROR_FILE_BACKEND_H
#define PINBRIDGE_ERROR_FILE_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendPinWriteErrorMessage(
    const char* message, int32_t type, PbPinErrorSeverity severity,
    const char* const* arguments, uint32_t argument_count);

#endif
