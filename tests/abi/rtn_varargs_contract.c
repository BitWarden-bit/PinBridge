#include <stdint.h>
#include <stdio.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    const PbRtnHandle routine = {91};
    PbIargListHandle list = (PbIargListHandle)(uintptr_t)0x2000;
    uint8_t inserted = 0;
    uint64_t original = 0;

    if (pb_rtn_insert_call(
            routine, PB_IPOINT_BEFORE, UINT64_C(0x1000), list) != PB_OK ||
        pb_rtn_insert_call_probed(
            routine, PB_IPOINT_AFTER, UINT64_C(0x1100), list,
            &inserted) != PB_OK || inserted != 1 ||
        pb_rtn_insert_call_probed_ex(
            routine, PB_IPOINT_BEFORE, PB_PROBE_MODE_ALLOW_RELOCATION,
            UINT64_C(0x1200), list, &inserted) != PB_OK || inserted != 1)
        return 1;

    if (pb_rtn_replace_signature(
            routine, UINT64_C(0x1300), list, &original) != PB_OK ||
        original != UINT64_C(0x4130)) {
        fprintf(stderr, "replace_signature original=0x%llx\n",
            (unsigned long long)original);
        return 2;
    }
    if (pb_rtn_replace_signature_probed(
            routine, UINT64_C(0x1400), list, &original) != PB_OK ||
        original != UINT64_C(0x4140)) {
        fprintf(stderr, "replace_signature_probed original=0x%llx\n",
            (unsigned long long)original);
        return 2;
    }
    if (pb_rtn_replace_signature_probed_ex(
            routine, PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET,
            UINT64_C(0x1500), list, &original) != PB_OK ||
        original != UINT64_C(0x4150)) {
        fprintf(stderr, "replace_signature_probed_ex original=0x%llx\n",
            (unsigned long long)original);
        return 2;
    }

    original = 7;
    inserted = 7;
    if (pb_rtn_insert_call(
            (PbRtnHandle){0}, PB_IPOINT_BEFORE, UINT64_C(0x1000), list) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_insert_call(routine, PB_IPOINT_ANYWHERE,
            UINT64_C(0x1000), list) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_insert_call(routine, PB_IPOINT_BEFORE, 0, list) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_insert_call(routine, PB_IPOINT_BEFORE, UINT64_C(0x1000),
            PB_IARG_LIST_INVALID) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_insert_call_probed(routine, PB_IPOINT_BEFORE,
            UINT64_C(0x1100), list, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_insert_call_probed_ex(routine, PB_IPOINT_BEFORE,
            (PbProbeMode)4, UINT64_C(0x1200), list, &inserted) !=
            PB_ERR_INVALID_ARGUMENT || inserted != 0 ||
        pb_rtn_replace_signature(routine, UINT64_C(0x1300), list, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_rtn_replace_signature_probed_ex(routine, (PbProbeMode)4,
            UINT64_C(0x1500), list, &original) != PB_ERR_INVALID_ARGUMENT ||
        original != 0)
        return 3;
    return 0;
}
