#ifndef PINBRIDGE_CALLBACKS_BACKEND_H
#define PINBRIDGE_CALLBACKS_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendCallbackGetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder* out_order);
PbStatus PbBackendCallbackSetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder order);

#endif
