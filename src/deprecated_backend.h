#ifndef PINBRIDGE_DEPRECATED_BACKEND_H
#define PINBRIDGE_DEPRECATED_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendCallbackGetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t* out_priority);
PbStatus PbBackendCallbackSetExecutionPriorityDeprecated(
    PbCallbackHandle callback, int32_t priority);
PbStatus PbBackendImgEntryDeprecated(PbImgHandle image, uint64_t* out_entry);

#endif
