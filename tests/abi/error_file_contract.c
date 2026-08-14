#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    const char* arguments[] = {"alpha", "beta"};
    if (PB_PIN_ERR_FATAL != 0 || PB_PIN_ERR_NONFATAL != 1 ||
        PB_PIN_ERR_NONE != 0 || PB_PIN_ERR_LAST != 56 ||
        PB_PIN_ERROR_TYPE_COUNT != 57u)
        return 1;
    if (pb_pin_write_error_message(
            "pinbridge mock error", 1001, PB_PIN_ERR_NONFATAL,
            arguments, 2) != PB_OK)
        return 2;
    if (pb_pin_write_error_message(
            "no arguments", 1002, PB_PIN_ERR_NONFATAL, 0, 0) != PB_OK)
        return 3;
    if (pb_pin_write_error_message(
            0, 1001, PB_PIN_ERR_NONFATAL, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_write_error_message(
            "bad type", 999, PB_PIN_ERR_NONFATAL, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_write_error_message(
            "bad severity", 1001, (PbPinErrorSeverity)2, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_write_error_message(
            "missing arguments", 1001, PB_PIN_ERR_NONFATAL, 0, 1) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_write_error_message(
            "too many", 1001, PB_PIN_ERR_NONFATAL, arguments,
            PB_PIN_ERROR_ARGUMENT_LIMIT + 1) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
