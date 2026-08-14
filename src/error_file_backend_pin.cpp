#include "pin.H"

#include "error_file_backend.h"

PbStatus PbBackendPinWriteErrorMessage(
    const char* message, int32_t type, PbPinErrorSeverity severity,
    const char* const* arguments, uint32_t argument_count)
{
    const PIN_ERR_SEVERITY_TYPE pin_severity =
        static_cast<PIN_ERR_SEVERITY_TYPE>(severity);
    switch (argument_count)
    {
    case 0:
        PIN_WriteErrorMessage(message, type, pin_severity, 0);
        break;
    case 1:
        PIN_WriteErrorMessage(message, type, pin_severity, 1, arguments[0]);
        break;
    case 2:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 2, arguments[0], arguments[1]);
        break;
    case 3:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 3, arguments[0], arguments[1],
            arguments[2]);
        break;
    case 4:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 4, arguments[0], arguments[1],
            arguments[2], arguments[3]);
        break;
    case 5:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 5, arguments[0], arguments[1],
            arguments[2], arguments[3], arguments[4]);
        break;
    case 6:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 6, arguments[0], arguments[1],
            arguments[2], arguments[3], arguments[4], arguments[5]);
        break;
    case 7:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 7, arguments[0], arguments[1],
            arguments[2], arguments[3], arguments[4], arguments[5],
            arguments[6]);
        break;
    case 8:
        PIN_WriteErrorMessage(
            message, type, pin_severity, 8, arguments[0], arguments[1],
            arguments[2], arguments[3], arguments[4], arguments[5],
            arguments[6], arguments[7]);
        break;
    default:
        return PB_ERR_INVALID_ARGUMENT;
    }
    return PB_OK;
}
