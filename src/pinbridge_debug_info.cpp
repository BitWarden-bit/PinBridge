#include "pinbridge/pinbridge.h"

#include "debug_info_backend.h"

PbStatus PB_CALL pb_pin_get_source_location(
    uint64_t address, int32_t* column, int32_t* line,
    char* file_name, uint64_t capacity, uint64_t* required_size)
{
    if (!required_size || (!file_name && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendGetSourceLocation(
            address, column, line, file_name, capacity, required_size);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}
