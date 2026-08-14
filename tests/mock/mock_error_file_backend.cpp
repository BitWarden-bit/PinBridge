#include "error_file_backend.h"

#include <cstring>

PbStatus PbBackendPinWriteErrorMessage(
    const char* message, int32_t type, PbPinErrorSeverity severity,
    const char* const* arguments, uint32_t argument_count)
{
    if (severity != PB_PIN_ERR_NONFATAL)
        return PB_ERR_INTERNAL;
    if (std::strcmp(message, "pinbridge mock error") == 0)
    {
        return type == 1001 && argument_count == 2 &&
            std::strcmp(arguments[0], "alpha") == 0 &&
            std::strcmp(arguments[1], "beta") == 0 ? PB_OK : PB_ERR_INTERNAL;
    }
    if (std::strcmp(message, "no arguments") == 0)
        return type == 1002 && argument_count == 0 ? PB_OK : PB_ERR_INTERNAL;
    return PB_ERR_INTERNAL;
}
