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

    if (!Expect(pb_string_bool(1, buffer, sizeof(buffer), &required),
                buffer, "T") || required != 2 ||
        !Expect(pb_string_tri(PB_TRI_MAYBE, buffer, sizeof(buffer), &required),
                buffer, "M") ||
        !Expect(pb_string_dec(42, 5, '0', buffer, sizeof(buffer), &required),
                buffer, "00042") ||
        !Expect(pb_string_dec_signed(-42, 5, '0', buffer, sizeof(buffer), &required),
                buffer, "00-42"))
        return 1;
    if (!Expect(pb_string_bignum(1234567, 0, ' ', buffer, sizeof(buffer), &required),
                buffer, "1,234,567") ||
        pb_string_flt(3.5, 2, 0, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_string_from_addrint(0x2a, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_string_from_uint64(0x2a, buffer, sizeof(buffer), &required) != PB_OK)
        return 2;
    if (pb_string_hex(0x2a, 4, 1, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_string_hex32(0x2a, 4, 1, buffer, sizeof(buffer), &required) != PB_OK ||
        pb_pointer_string((const void*)(uintptr_t)0x1234,
                          buffer, sizeof(buffer), &required) != PB_OK)
        return 3;
    if (!Expect(pb_left_justify("ab", 4, '.', buffer, sizeof(buffer), &required),
                buffer, "ab..") ||
        pb_reformat("alpha beta", "> ", 4, 8,
                    buffer, sizeof(buffer), &required) != PB_OK)
        return 4;

    buffer[0] = 'X';
    if (pb_string_bool(0, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != 2 ||
        pb_string_bool(0, buffer, 1, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        buffer[0] != 'X' ||
        pb_string_bool(0, buffer, sizeof(buffer), 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_left_justify(0, 1, ' ', buffer, sizeof(buffer), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_string_tri((PbTri)99, buffer, sizeof(buffer), &required) !=
            PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
