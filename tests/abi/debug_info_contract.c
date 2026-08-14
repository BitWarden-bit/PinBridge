#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    int32_t column = 0;
    int32_t line = 0;
    uint64_t required = 0;
    char file_name[32] = {0};
    char tiny[2] = {'X', 0};

    if (pb_pin_get_source_location(
            UINT64_C(0x401000), &column, &line, 0, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        column != 7 || line != 42 || required != strlen("mock/source.c") + 1u)
        return 1;
    if (pb_pin_get_source_location(
            UINT64_C(0x401000), &column, &line,
            file_name, sizeof(file_name), &required) != PB_OK ||
        strcmp(file_name, "mock/source.c") != 0)
        return 2;
    if (pb_pin_get_source_location(
            UINT64_C(0x401000), 0, 0, tiny, sizeof(tiny), &required) !=
            PB_ERR_BUFFER_TOO_SMALL || tiny[0] != 'X')
        return 3;
    if (pb_pin_get_source_location(0, 0, 0, 0, 1, &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_source_location(0, 0, 0, file_name, sizeof(file_name), 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
