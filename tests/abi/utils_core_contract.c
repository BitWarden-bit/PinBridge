#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    uint8_t space = 0;
    int32_t digit = 0;
    char upper = 0;
    double floating = 0.0;
    int32_t signed32 = 0;
    int64_t signed64 = 0;
    uint32_t unsigned32 = 0;
    uint64_t unsigned64 = 0;
    uint64_t address = 0;
    uint64_t difference = 0;
    uint32_t bits = 0;
    void* pointer = 0;
    const void* const_pointer = 0;
    const void* stack_pointer = 0;
    uint64_t rounded = 0;
    uint8_t storage[32] = {0};

    if (pb_addrint_to_pointer(UINT64_C(0x1234), &pointer) != PB_OK ||
        (uintptr_t)pointer != (uintptr_t)0x1234 ||
        pb_pointer_to_addrint(pointer, &address) != PB_OK ||
        address != UINT64_C(0x1234) ||
        pb_const_pointer_to_addrint(pointer, &address) != PB_OK ||
        address != UINT64_C(0x1234))
        return 1;
    if (pb_addrint_from_string("0x2a", &address) != PB_OK || address != 42 ||
        pb_int32_from_string("-17", &signed32) != PB_OK || signed32 != -17 ||
        pb_int64_from_string("-4000000000", &signed64) != PB_OK ||
        signed64 != INT64_C(-4000000000) ||
        pb_uint32_from_string("19", &unsigned32) != PB_OK || unsigned32 != 19 ||
        pb_uint64_from_string("5000000000", &unsigned64) != PB_OK ||
        unsigned64 != UINT64_C(5000000000) ||
        pb_flt64_from_string("3.5", &floating) != PB_OK || floating != 3.5)
        return 2;
    if (pb_bit_count(UINT64_C(0xf0f0), &bits) != PB_OK || bits != 8 ||
        pb_char_is_space(' ', &space) != PB_OK || !space ||
        pb_char_to_hex_digit('B', &digit) != PB_OK || digit != 11 ||
        pb_char_to_upper('q', &upper) != PB_OK || upper != 'Q')
        return 3;
    if (pb_get_page_of_addr(UINT64_C(0x12345), &address) != PB_OK ||
        address != UINT64_C(0x12000) ||
        pb_get_sp(&stack_pointer) != PB_OK || !stack_pointer)
        return 4;
    if (pb_ptr_at_offset(storage, 7, &pointer) != PB_OK ||
        pointer != storage + 7 ||
        pb_const_ptr_at_offset(storage, 11, &const_pointer) != PB_OK ||
        const_pointer != storage + 11 ||
        pb_ptr_diff(storage + 20, storage + 3, &difference) != PB_OK ||
        difference != 17)
        return 5;

    if (pb_round_up_u64(UINT64_C(0x1235), 0x1000, &rounded) != PB_OK ||
        rounded != UINT64_C(0x2000) ||
        pb_round_down_u64(UINT64_C(0x1235), 0x1000, &rounded) != PB_OK ||
        rounded != UINT64_C(0x1000) ||
        pb_round_up_addr(UINT64_C(0x1235), 0x1000, &rounded) != PB_OK ||
        rounded != UINT64_C(0x2000) ||
        pb_round_down_addr(UINT64_C(0x1235), 0x1000, &rounded) != PB_OK ||
        rounded != UINT64_C(0x1000))
        return 7;
    if (pb_round_up_u64(UINT64_C(0x1235), 0, &rounded) != PB_OK ||
        rounded != UINT64_C(0x1235) ||
        pb_round_down_addr(UINT64_C(0x1235), 0, &rounded) != PB_OK ||
        rounded != UINT64_C(0x1235))
        return 8;
    
    if (pb_addrint_to_pointer(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_addrint_from_string(0, &address) != PB_ERR_INVALID_ARGUMENT ||
        pb_bit_count(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_char_is_space(' ', 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ptr_at_offset(storage, 1, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ptr_diff(storage, storage, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_round_up_u64(1, 8, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_round_down_addr(1, 8, 0) != PB_ERR_INVALID_ARGUMENT)
        return 6;
    return 0;
}
