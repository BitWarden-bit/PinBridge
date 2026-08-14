#include "pinbridge/pinbridge.h"

#include "control_memory_translation_backend.h"

namespace
{

template< typename Function > PbStatus GuardMemoryTranslation(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_add_memory_address_trans_function(
    PbMemoryAddressTransCallback callback, void* user_data)
{
    if (!callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardMemoryTranslation(
        [&]() { return PbBackendAddMemoryAddressTransFunction(callback, user_data); });
}

PbStatus PB_CALL pb_pin_get_memory_address_trans_function(
    PbMemoryAddressTransCallback* out_callback)
{
    if (!out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    *out_callback = 0;
    return GuardMemoryTranslation(
        [&]() { return PbBackendGetMemoryAddressTransFunction(out_callback); });
}
