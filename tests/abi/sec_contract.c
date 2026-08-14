#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

_Static_assert(sizeof(PbSecType) == 4, "PbSecType must be 32-bit");
_Static_assert(PB_SEC_TYPE_INVALID == 0u, "SEC_TYPE_INVALID changed");
_Static_assert(PB_SEC_TYPE_LAST == 26u, "SEC_TYPE_LAST changed");
_Static_assert(PB_SEC_TYPE_COUNT == 27u, "SEC_TYPE coverage is incomplete");

int main(void)
{
    const PbSecHandle sec = {7};
    const PbSecHandle invalid_input = {0};
    PbSecHandle invalid_sec = {99};
    uint64_t address = 0;
    uint64_t required = 0;
    char name[32] = {0};
    char tiny[2] = {'X', 0};

    if (pb_sec_invalid(&invalid_sec) != PB_OK || invalid_sec.opaque != 0)
        return 1;
    if (pb_sec_data(sec, &address) != PB_OK || address != UINT64_C(0x10000007))
        return 2;
    if (pb_sec_name(sec, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != strlen("mock-section-7") + 1u)
        return 3;
    if (pb_sec_name(sec, name, sizeof(name), &required) != PB_OK ||
        strcmp(name, "mock-section-7") != 0)
        return 4;
    if (pb_sec_name(sec, tiny, sizeof(tiny), &required) != PB_ERR_BUFFER_TOO_SMALL ||
        tiny[0] != 'X')
        return 5;
    if (pb_sec_data(invalid_input, &address) != PB_ERR_INVALID_ARGUMENT ||
        pb_sec_data(sec, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_sec_invalid(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_sec_name(invalid_input, name, sizeof(name), &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_sec_name(sec, 0, 1, &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_sec_name(sec, name, sizeof(name), 0) != PB_ERR_INVALID_ARGUMENT)
        return 6;
    return 0;
}
