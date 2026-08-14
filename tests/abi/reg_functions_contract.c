#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbRegId claimed = PB_REG_INVALID_;
    PbRegId reg = PB_REG_RAX;
    PbRegId result = 0;
    PbFxSave fxsave = {{0}};
    uint16_t full_tag = 0;
    uint64_t required = 0;
    char text[64];

    fxsave.bytes[4] = 0x5au;
    if (pb_pin_claim_tool_register(&claimed) != PB_OK || claimed == PB_REG_INVALID_)
        return 1;
    if (pb_reg_convert_x87_abridged_tag_to_full(&fxsave, &full_tag) != PB_OK)
        return 2;
    if (pb_reg_string_short(PB_REG_RAX, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required < 2 || required > sizeof(text) ||
        pb_reg_string_short(PB_REG_RAX, text, sizeof(text), &required) != PB_OK ||
        text[required - 1] != '\0')
        return 3;
    if (pb_reg_prefix_increment(&reg, &result) != PB_OK ||
        reg != PB_REG_RAX + 1u || result != reg)
        return 4;
    if (pb_reg_postfix_increment(&reg, &result) != PB_OK ||
        result != PB_REG_RAX + 1u || reg != PB_REG_RAX + 2u)
        return 5;
    if (pb_reg_postfix_decrement(&reg, &result) != PB_OK ||
        result != PB_REG_RAX + 2u || reg != PB_REG_RAX + 1u)
        return 6;
    if (pb_pin_claim_tool_register(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_convert_x87_abridged_tag_to_full(0, &full_tag) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_convert_x87_abridged_tag_to_full(&fxsave, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_string_short(PB_REG_LAST, text, sizeof(text), &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_prefix_increment(0, &result) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_postfix_increment(&reg, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_reg_postfix_decrement(0, &result) != PB_ERR_INVALID_ARGUMENT)
        return 7;
    reg = PB_REG_LAST;
    if (pb_reg_prefix_increment(&reg, &result) != PB_ERR_INVALID_ARGUMENT)
        return 8;
    reg = PB_REG_INVALID_;
    if (pb_reg_postfix_decrement(&reg, &result) != PB_ERR_INVALID_ARGUMENT)
        return 9;
    return 0;
}
