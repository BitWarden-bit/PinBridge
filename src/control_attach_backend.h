#ifndef PINBRIDGE_CONTROL_ATTACH_BACKEND_H
#define PINBRIDGE_CONTROL_ATTACH_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAttach(
    PbAttachCallback callback, void* user_data, PbAttachStatus* out_status);
PbStatus PbBackendAttachProbed(
    PbAttachProbedCallback callback, void* user_data, PbAttachStatus* out_status);

#endif
