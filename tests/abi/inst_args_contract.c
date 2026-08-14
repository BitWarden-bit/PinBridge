#include <stddef.h>
#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbIargListHandle list = PB_IARG_LIST_INVALID;
    PbIargDescriptor descriptors[4] = {
        {PB_IARG_PTR, 0, UINT64_C(0x1234), 0},
        {PB_IARG_THREAD_ID, 0, 0, 0},
        {PB_IARG_PROTOTYPE, 0, UINT64_C(0x2345), 0},
        {PB_IARG_PARTIAL_CONTEXT, 0, UINT64_C(0x3456), UINT64_C(0x4567)},
    };

    if (sizeof(PbIargType) != 4 || sizeof(PbIpoint) != 4 ||
        sizeof(PbPinMemop) != 4 || sizeof(PbPinOpElementAccess) != 4 ||
        sizeof(PbIargDescriptor) != 24 ||
        offsetof(PbIargDescriptor, value) != 8 ||
        offsetof(PbIargDescriptor, value2) != 16 ||
        PB_MAX_MULTI_MEMOPS != 16 || PB_IARG_END != PB_IARG_LAST)
        return 1;

    if (pb_iarg_list_alloc(&list) != PB_OK || list == PB_IARG_LIST_INVALID)
        return 2;
    if (pb_iarg_list_add(list, descriptors, 4) != PB_OK)
        return 3;
    if (pb_iarg_list_free(list) != PB_OK)
        return 4;

    if (pb_iarg_list_alloc(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_iarg_list_add(PB_IARG_LIST_INVALID, descriptors, 1) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_iarg_list_add((PbIargListHandle)(uintptr_t)1, 0, 1) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_iarg_list_add((PbIargListHandle)(uintptr_t)1, descriptors, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_iarg_list_free(PB_IARG_LIST_INVALID) != PB_ERR_INVALID_ARGUMENT)
        return 5;

    descriptors[0].reserved = 1;
    if (pb_iarg_list_add((PbIargListHandle)(uintptr_t)1, descriptors, 1) !=
        PB_ERR_INVALID_ARGUMENT)
        return 6;
    descriptors[0].reserved = 0;
    descriptors[0].type = PB_IARG_INVALID;
    if (pb_iarg_list_add((PbIargListHandle)(uintptr_t)1, descriptors, 1) !=
        PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
