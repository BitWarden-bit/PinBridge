#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

static int Expect(PbStatus status, const char* actual, const char* expected)
{
    return status == PB_OK && strcmp(actual, expected) == 0;
}

int main(void)
{
    char buffer[128] = {0};
    uint64_t required = 0;
    uint8_t storage[8] = {0};
    const void* const_pointer = storage;

    if (!Expect(pb_decstr_i16(-12, 5, buffer, sizeof(buffer), &required),
                buffer, "  -12") ||
        !Expect(pb_decstr_i32(-123, 6, buffer, sizeof(buffer), &required),
                buffer, "  -123") ||
        pb_decstr_i64(INT64_C(-1234), 7,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_decstr_u16(UINT16_C(12), 4,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_decstr_u32(UINT32_C(123), 5,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_decstr_u64(UINT64_C(1234), 6,
                     buffer, sizeof(buffer), &required) != PB_OK)
        return 1;

    if (pb_fltstr(3.5, 2, 6, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_i16(-1, 4, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_i32(-2, 8, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_i64(INT64_C(-3), 16,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_u16(UINT16_C(0x12), 4,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_u32(UINT32_C(0x1234), 8,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_u64(UINT64_C(0x12345678), 16,
                     buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_pointer(storage, 0,
                          buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_const_pointer(const_pointer, 0,
                                buffer, sizeof(buffer), &required) != PB_OK)
        return 2;

    buffer[0] = 'X';
    if (pb_decstr_i16(0, 0, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != 2 ||
        pb_decstr_i16(0, 0, buffer, 1, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        buffer[0] != 'X' ||
        pb_decstr_i16(0, 0, buffer, sizeof(buffer), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_hexstr_pointer(0, 0, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_hexstr_const_pointer(0, 0,
                                buffer, sizeof(buffer), &required) != PB_OK)
        return 3;

    return 0;
}
