#ifndef PINBRIDGE_CONTROL_INSERT_CALL_PROBED_BACKEND_H
#define PINBRIDGE_CONTROL_INSERT_CALL_PROBED_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendInsertCallProbed(
    uint64_t address, PbProbedCallCallback callback,
    void* user_data, uint8_t* out_inserted);

#endif
