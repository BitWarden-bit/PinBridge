#include <stdint.h>

#include "pinbridge/pinbridge.h"

static int expect_empty(const PbRegSet* set, uint8_t expected)
{
    uint8_t actual = 2;
    return pb_regset_is_empty(set, &actual) == PB_OK && actual == expected;
}

int main(void)
{
    PbRegSet set = {{0}};
    char text[256];
    uint8_t contains = 0;
    uint32_t count = 0;
    uint64_t required = 0;
    PbRegId first = 0;
    PbRegId last = 0;
    PbRegId popped = 0;

    if (!expect_empty(&set, 1))
        return 1;
    if (pb_regset_insert(&set, 7) != PB_OK ||
        pb_regset_insert(&set, 200) != PB_OK)
        return 2;
    if (pb_regset_contains(&set, 7, &contains) != PB_OK || contains != 1)
        return 3;
    if (pb_regset_pop_count(&set, &count) != PB_OK || count != 2)
        return 4;
    if (pb_regset_pop_next(&set, &popped) != PB_OK || popped != 7)
        return 5;
    if (pb_regset_remove(&set, 200) != PB_OK || !expect_empty(&set, 1))
        return 6;

    if (pb_regset_add_all(&set) != PB_OK ||
        pb_regset_pop_count(&set, &count) != PB_OK || count == 0)
        return 7;
    if (pb_regset_clear(&set) != PB_OK || !expect_empty(&set, 1))
        return 8;

    if (pb_regset_clear(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_insert(0, 7) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_contains(0, 7, &contains) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_contains(&set, 7, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_pop_count(0, &count) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_pop_count(&set, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_is_empty(0, &contains) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_is_empty(&set, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_pop_next(0, &popped) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_pop_next(&set, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_remove(0, 7) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_insert(&set, PB_REGSET_MAX_REG_ID + 1u) != PB_ERR_INVALID_ARGUMENT)
        return 9;

    if (pb_regset_first_reg(&first) != PB_OK ||
        pb_regset_last_reg(&last) != PB_OK || first > last)
        return 10;
    if (pb_regset_first_reg(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_regset_last_reg(0) != PB_ERR_INVALID_ARGUMENT)
        return 11;
    if (pb_regset_insert(&set, first) != PB_OK ||
        pb_regset_string_short(&set, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required < 2 || required > sizeof(text))
        return 12;
    if (pb_regset_string_short(&set, text, required - 1, &required) !=
        PB_ERR_BUFFER_TOO_SMALL)
        return 13;
    if (pb_regset_string_short(&set, text, sizeof(text), &required) != PB_OK ||
        text[required - 1] != '\0')
        return 14;
    if (pb_regset_string_short(0, text, sizeof(text), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_regset_string_short(&set, text, sizeof(text), 0) != PB_ERR_INVALID_ARGUMENT)
        return 15;

    return 0;
}
