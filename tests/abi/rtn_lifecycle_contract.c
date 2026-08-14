#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbRtnHandle routine = {81};
    PbRtnHandle created = {99};

    if (pb_rtn_open(routine) != PB_OK ||
        pb_rtn_open(routine) != PB_ERR_INVALID_STATE ||
        pb_rtn_create_at(
            UINT64_C(0x1234), "mock_created", &created) !=
            PB_ERR_INVALID_STATE ||
        created.opaque != 0 ||
        pb_rtn_close((PbRtnHandle){82}) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_close(routine) != PB_OK ||
        pb_rtn_close(routine) != PB_ERR_INVALID_STATE)
        return 1;

    if (pb_rtn_create_at(
            UINT64_C(0x1234), "mock_created", &created) != PB_OK ||
        created.opaque != 82)
        return 2;

    created.opaque = 99;
    if (pb_rtn_create_at(0, "mock_created", &created) !=
            PB_ERR_PIN_REJECTED_ARGUMENTS ||
        created.opaque != 0)
        return 3;

    if (pb_rtn_open((PbRtnHandle){0}) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_close((PbRtnHandle){0}) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_create_at(UINT64_C(0x1234), 0, &created) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_create_at(UINT64_C(0x1234), "", &created) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_create_at(UINT64_C(0x1234), "mock_created", 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 4;

    return 0;
}
