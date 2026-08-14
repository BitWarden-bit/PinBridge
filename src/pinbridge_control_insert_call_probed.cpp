#include "pinbridge/pinbridge.h"

#include "control_insert_call_probed_backend.h"

PbStatus PB_CALL pb_pin_insert_call_probed(
    uint64_t address, PbProbedCallCallback callback,
    void* user_data, uint8_t* out_inserted)
{
    if (out_inserted)
        *out_inserted = 0;
    if (address == 0 || !callback || !out_inserted)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendInsertCallProbed(
            address, callback, user_data, out_inserted);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}
