#include "pinbridge/pinbridge.h"

#include "control_out_of_memory_backend.h"

namespace
{

template< typename Function > PbStatus GuardOutOfMemoryRegistration(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_add_out_of_memory_function(
    PbOutOfMemoryCallback callback, void* user_data)
{
    return GuardOutOfMemoryRegistration([&]() {
        return PbBackendAddOutOfMemoryFunction(callback, user_data);
    });
}
