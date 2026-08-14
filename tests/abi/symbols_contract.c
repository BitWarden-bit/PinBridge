#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    const PbSymHandle valid = {91};
    const PbSymHandle invalid = {0};
    PbSymHandle symbol = {99};
    uint64_t value64 = 0;
    uint64_t required = 0;
    uint32_t value32 = 0;
    uint8_t value8 = 0;
    char buffer[64];

    if (sizeof(PbUndecoration) != 4 ||
        PB_UNDECORATION_COMPLETE != 0 ||
        PB_UNDECORATION_NAME_ONLY != 1)
        return 1;
    if (pb_sym_invalid(&symbol) != PB_OK || symbol.opaque != 0 ||
        pb_sym_valid(valid, &value8) != PB_OK || value8 != 1 ||
        pb_sym_valid(invalid, &value8) != PB_OK || value8 != 0)
        return 2;
    if (pb_sym_address(valid, &value64) != PB_OK ||
        value64 != UINT64_C(0x140001000) ||
        pb_sym_value(valid, &value64) != PB_OK || value64 != UINT64_C(0x1000) ||
        pb_sym_index(valid, &value32) != PB_OK || value32 != 7)
        return 3;
    if (pb_sym_dynamic(valid, &value8) != PB_OK || value8 != 1 ||
        pb_sym_generated_by_pin(valid, &value8) != PB_OK || value8 != 0)
        return 4;
    if (pb_sym_next(valid, &symbol) != PB_OK || symbol.opaque != 92 ||
        pb_sym_prev(valid, &symbol) != PB_OK || symbol.opaque != 90)
        return 5;
    if (pb_sym_name(valid, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != sizeof("mock_symbol") ||
        pb_sym_name(valid, buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "mock_symbol") != 0)
        return 6;
    if (pb_pin_undecorate_symbol_name(
            "?PbSymbolFixture@@YAHH@Z", PB_UNDECORATION_NAME_ONLY,
            0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != sizeof("PbSymbolFixture") ||
        pb_pin_undecorate_symbol_name(
            "?PbSymbolFixture@@YAHH@Z", PB_UNDECORATION_NAME_ONLY,
            buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "PbSymbolFixture") != 0)
        return 7;
    if (pb_sym_address(invalid, &value64) != PB_ERR_INVALID_ARGUMENT ||
        pb_sym_name(valid, 0, 1, &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_sym_next(valid, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_sym_valid(valid, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_undecorate_symbol_name(
            0, PB_UNDECORATION_COMPLETE, buffer, sizeof(buffer),
            &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_undecorate_symbol_name(
            "x", (PbUndecoration)2, buffer, sizeof(buffer),
            &required) != PB_ERR_INVALID_ARGUMENT)
        return 8;
    return 0;
}
