#ifndef PINBRIDGE_CONTROL_PROBE_DETACH_BACKEND_H
#define PINBRIDGE_CONTROL_PROBE_DETACH_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddDetachFunctionProbed(
    PbDetachProbedCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendDetach(void);
PbStatus PbBackendDetachProbed(void);

#endif
