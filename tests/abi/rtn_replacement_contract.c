#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbRtnHandle routine = {81};
    uint64_t original = 99;

    if (sizeof(PbProbeMode) != 4 ||
        PB_PROBE_MODE_DEFAULT != 0 ||
        PB_PROBE_MODE_ALLOW_RELOCATION != 1 ||
        PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET != 2)
        return 1;

    if (pb_rtn_replace(
            routine, UINT64_C(0x4000), &original) != PB_OK ||
        original != UINT64_C(0x1234) ||
        pb_rtn_replace_probed(
            routine, UINT64_C(0x5000), &original) != PB_OK ||
        original != UINT64_C(0x2234) ||
        pb_rtn_replace_probed_ex(
            routine, PB_PROBE_MODE_ALLOW_RELOCATION |
                PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET,
            UINT64_C(0x6000), &original) != PB_OK ||
        original != UINT64_C(0x3234))
        return 2;

    original = 99;
    if (pb_rtn_replace(
            (PbRtnHandle){82}, UINT64_C(0x4000), &original) !=
            PB_ERR_PIN_REJECTED_ARGUMENTS ||
        original != 0)
        return 3;

    if (pb_rtn_replace((PbRtnHandle){0}, UINT64_C(0x4000), &original) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace(routine, 0, &original) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace(routine, UINT64_C(0x4000), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed((PbRtnHandle){0}, UINT64_C(0x5000), &original) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed(routine, 0, &original) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed(routine, UINT64_C(0x5000), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed_ex(
            routine, (PbProbeMode)4, UINT64_C(0x6000), &original) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed_ex(
            routine, PB_PROBE_MODE_DEFAULT, 0, &original) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_probed_ex(
            routine, PB_PROBE_MODE_DEFAULT, UINT64_C(0x6000), 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 4;

    return 0;
}
