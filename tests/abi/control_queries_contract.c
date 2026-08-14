#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    uint8_t value = 0;
    uint64_t required = 0;
    char path[64] = {0};

    if (pb_pin_check_read_access(UINT64_C(0x1000), &value) != PB_OK || value != 1u)
        return 1;
    if (pb_pin_check_write_access(UINT64_C(0x1000), &value) != PB_OK || value != 0u)
        return 2;
    if (pb_pin_is_attaching(&value) != PB_OK || value != 0u)
        return 3;
    if (pb_pin_is_probe_mode(&value) != PB_OK || value != 0u)
        return 4;
    if (pb_pin_is_safe_for_probed_insertion(UINT64_C(0x401000), &value) !=
        PB_ERR_INVALID_STATE)
        return 5;

    if (pb_pin_tool_full_path(0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != strlen("C:\\mock\\pinbridge.dll") + 1u ||
        pb_pin_tool_full_path(path, sizeof(path), &required) != PB_OK ||
        strcmp(path, "C:\\mock\\pinbridge.dll") != 0)
        return 6;

    if (pb_pin_check_read_access(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_check_write_access(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_is_attaching(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_is_probe_mode(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_is_safe_for_probed_insertion(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_tool_full_path(0, 1, &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_tool_full_path(path, sizeof(path), 0) != PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
