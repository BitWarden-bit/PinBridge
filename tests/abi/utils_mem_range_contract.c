#include <stddef.h>
#include <stdint.h>

#include "pinbridge/pinbridge.h"

_Static_assert(sizeof(PbMemRange) == 16, "PbMemRange must be 16 bytes");
_Static_assert(offsetof(PbMemRange, base) == 0, "PbMemRange base offset changed");
_Static_assert(offsetof(PbMemRange, size) == 8, "PbMemRange size offset changed");

int main(void)
{
    PbMemRange range = {0, 0};
    if (pb_mem_page_range_addr(UINT64_C(0x12345), &range) != PB_OK ||
        range.base != UINT64_C(0x12000) || range.size != UINT64_C(0x1000))
        return 1;
    if (pb_mem_page_range_pointer((const void*)(uintptr_t)0x12345, &range) != PB_OK ||
        range.base != UINT64_C(0x12000) || range.size != UINT64_C(0x1000))
        return 2;
    if (pb_mem_page_range_pointer(0, &range) != PB_OK ||
        range.base != 0 || range.size != UINT64_C(0x1000))
        return 3;
    if (pb_mem_page_range_addr(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_mem_page_range_pointer(0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
