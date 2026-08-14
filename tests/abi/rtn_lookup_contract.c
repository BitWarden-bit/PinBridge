#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbImgHandle image = {76};
    PbRtnHandle routine = {0};
    char buffer[64] = {0};
    uint64_t required = 0;
    uint64_t function_address = 0;

    if (pb_rtn_find_by_address(UINT64_C(0x1234), &routine) != PB_OK ||
        routine.opaque != 81 ||
        pb_rtn_find_by_name(image, "mock_routine", &routine) != PB_OK ||
        routine.opaque != 81 ||
        pb_rtn_funptr(routine, &function_address) != PB_OK ||
        function_address != UINT64_C(0x1234))
        return 1;

    if (pb_rtn_name(routine, 0, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        required != 13 ||
        pb_rtn_name(routine, buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "mock_routine") != 0 ||
        pb_rtn_find_name_by_address(
            UINT64_C(0x1234), buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "mock_routine") != 0)
        return 2;

    routine.opaque = 1;
    if (pb_rtn_invalid(&routine) != PB_OK || routine.opaque != 0)
        return 3;

    buffer[0] = 'X';
    if (pb_rtn_find_name_by_address(0, 0, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        required != 1 ||
        pb_rtn_find_name_by_address(0, buffer, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        buffer[0] != 'X' ||
        pb_rtn_find_name_by_address(0, buffer, sizeof(buffer), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_find_by_address(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_find_by_name((PbImgHandle){0}, "mock_routine", &routine) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_find_by_name(image, 0, &routine) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_find_by_name(image, "", &routine) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_find_by_name(image, "mock_routine", 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_funptr((PbRtnHandle){0}, &function_address) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_funptr((PbRtnHandle){81}, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_invalid(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_name((PbRtnHandle){0}, buffer, sizeof(buffer), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_name((PbRtnHandle){81}, 0, 1, &required) !=
            PB_ERR_INVALID_ARGUMENT)
        return 4;

    return 0;
}
