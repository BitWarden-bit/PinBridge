#include "control_insert_call_probed_backend.h"

PbStatus PbBackendInsertCallProbed(
    uint64_t address, PbProbedCallCallback callback,
    void* user_data, uint8_t* out_inserted)
{
    if (address != UINT64_C(0x123456789abcdef0))
        return PB_ERR_INTERNAL;
    *out_inserted = 1;
    callback(user_data);
    return PB_OK;
}
