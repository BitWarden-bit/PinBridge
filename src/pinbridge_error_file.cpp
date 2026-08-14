#include "pinbridge/pinbridge.h"

#include "error_file_backend.h"

PbStatus PB_CALL pb_pin_write_error_message(
    const char* message, int32_t type, PbPinErrorSeverity severity,
    const char* const* arguments, uint32_t argument_count)
{
    if (!message || type < 1000 ||
        (severity != PB_PIN_ERR_FATAL && severity != PB_PIN_ERR_NONFATAL) ||
        argument_count > PB_PIN_ERROR_ARGUMENT_LIMIT ||
        (argument_count != 0 && !arguments))
        return PB_ERR_INVALID_ARGUMENT;
    for (uint32_t index = 0; index < argument_count; ++index)
    {
        if (!arguments[index])
            return PB_ERR_INVALID_ARGUMENT;
    }
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return PbBackendPinWriteErrorMessage(
            message, type, severity, arguments, argument_count);
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return PbBackendPinWriteErrorMessage(
        message, type, severity, arguments, argument_count);
#endif
}
