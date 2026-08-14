#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

static int CheckString(PbStatus (PB_CALL* function)(
    uint32_t, char*, uint64_t, uint64_t*), uint32_t value,
    const char* expected)
{
    char buffer[64] = {0};
    uint64_t required = 0;
    if (function(value, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL ||
        required != strlen(expected) + 1u)
        return 0;
    if (function(value, buffer, required - 1u, &required) !=
            PB_ERR_BUFFER_TOO_SMALL || buffer[0] != '\0')
        return 0;
    return function(value, buffer, sizeof(buffer), &required) == PB_OK &&
        strcmp(buffer, expected) == 0;
}

int main(void)
{
    PbInsHandle ins = {7};
    PbInsHandle invalid = {7};
    PbXedDecodedInstHandle decoded = 0;
    PbRegId pin_reg = PB_REG_INVALID_;
    PbXedRegId xed_reg = 0;
    int32_t accesses = 0;
    int32_t access_size = 0;
    int32_t index_size = 0;
    uint16_t segment = 0;
    uint32_t displacement = 0;
    uint8_t changed = 0;
    char buffer[64] = {0};
    uint64_t required = 0;

    _Static_assert(sizeof(PbMemoryType) == 4, "PbMemoryType must be 32-bit");
    _Static_assert(sizeof(PbPredicate) == 4, "PbPredicate must be 32-bit");
    _Static_assert(sizeof(PbXedRegId) == 4, "PbXedRegId must be 32-bit");
    _Static_assert(PB_MEMORY_TYPE_READ == 0u && PB_MEMORY_TYPE_READ2 == 2u,
                   "MEMORY_TYPE values changed");
    _Static_assert(PB_PREDICATE_ALWAYS_TRUE == 0u &&
                   PB_PREDICATE_LAST == 22u, "PREDICATE values changed");
    _Static_assert(PB_VSYSCALL_NR == UINT32_C(0xABCDDCBA),
                   "VSYSCALL_NR changed");

    if (!CheckString(pb_category_string_short, 1, "mock_category") ||
        !CheckString(pb_extension_string_short, 2, "mock_extension") ||
        !CheckString(pb_opcode_string_short, 3, "mock_opcode"))
        return 1;
    if (pb_ins_disassemble(ins, buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "mock_disassembly") != 0 ||
        pb_ins_mnemonic(ins, buffer, sizeof(buffer), &required) != PB_OK ||
        strcmp(buffer, "mock_mnemonic") != 0)
        return 2;
    if (pb_ins_get_number_and_size_of_mem_accesses(
            ins, &accesses, &access_size, &index_size) != PB_OK ||
        accesses != 2 || access_size != 8 || index_size != 4)
        return 3;
    if (pb_ins_change_reg(
            ins, PB_REG_RAX, PB_REG_RBX, 1, &changed) != PB_OK || !changed)
        return 4;
    if (pb_ins_get_far_pointer(ins, &segment, &displacement) != PB_OK ||
        segment != 0x33u || displacement != UINT32_C(0x12345678))
        return 5;
    if (pb_ins_invalid(&invalid) != PB_OK || invalid.opaque != 0)
        return 6;
    if (pb_ins_xed_dec(ins, &decoded) != PB_OK || !decoded ||
        pb_ins_xed_exact_map_from_pin_reg(PB_REG_RAX, &xed_reg) != PB_OK ||
        xed_reg != 10u ||
        pb_ins_xed_exact_map_to_pin_reg(xed_reg, &pin_reg) != PB_OK ||
        pin_reg != PB_REG_RAX ||
        pb_ins_xed_exact_map_to_pin_reg_legacy(10u, &pin_reg) != PB_OK ||
        pin_reg != PB_REG_RAX)
        return 7;
    if (pb_pin_set_syntax_att() != PB_OK ||
        pb_pin_set_syntax_intel() != PB_OK ||
        pb_pin_set_syntax_xed() != PB_OK)
        return 8;

    if (pb_ins_disassemble(ins, 0, 1, &required) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_change_reg(ins, PB_REG_RAX, PB_REG_RBX, 2, &changed) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_get_number_and_size_of_mem_accesses(ins, 0, &access_size,
                                                    &index_size) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_xed_dec(ins, 0) != PB_ERR_INVALID_ARGUMENT)
        return 9;
    ins.opaque = 0;
    if (pb_ins_mnemonic(ins, buffer, sizeof(buffer), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_get_far_pointer(ins, &segment, &displacement) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_change_reg(ins, PB_REG_RAX, PB_REG_RBX, 0, &changed) !=
            PB_ERR_INVALID_ARGUMENT)
        return 10;
    return 0;
}
