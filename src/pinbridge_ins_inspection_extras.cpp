#include "pinbridge/pinbridge.h"

#include "ins_inspection_extras_backend.h"

namespace
{

template< typename Function > PbStatus GuardExtras(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsValid(PbInsHandle ins) { return ins.opaque > 0; }

template< typename Function >
PbStatus CopyGlobalString(
    char* buffer, uint64_t capacity, uint64_t* required_size,
    Function function)
{
    if (!required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = 0;
    return GuardExtras([&]() -> PbStatus {
        *required_size = function(buffer, capacity);
        return buffer && capacity >= *required_size
            ? PB_OK : PB_ERR_BUFFER_TOO_SMALL;
    });
}

template< typename Function >
PbStatus CopyInsString(
    PbInsHandle ins, char* buffer, uint64_t capacity, uint64_t* required_size,
    Function function)
{
    if (!IsValid(ins))
        return PB_ERR_INVALID_ARGUMENT;
    return CopyGlobalString(buffer, capacity, required_size,
        [&](char* target, uint64_t size) { return function(ins, target, size); });
}

} // namespace

PbStatus PB_CALL pb_category_string_short(
    uint32_t category, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyGlobalString(buffer, capacity, required_size,
        [&](char* target, uint64_t size) {
            return PbBackendCategoryStringShort(category, target, size);
        });
}

PbStatus PB_CALL pb_extension_string_short(
    uint32_t extension, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyGlobalString(buffer, capacity, required_size,
        [&](char* target, uint64_t size) {
            return PbBackendExtensionStringShort(extension, target, size);
        });
}

PbStatus PB_CALL pb_opcode_string_short(
    uint32_t opcode, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyGlobalString(buffer, capacity, required_size,
        [&](char* target, uint64_t size) {
            return PbBackendOpcodeStringShort(opcode, target, size);
        });
}

PbStatus PB_CALL pb_ins_disassemble(
    PbInsHandle ins, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyInsString(
        ins, buffer, capacity, required_size, PbBackendInsDisassemble);
}

PbStatus PB_CALL pb_ins_mnemonic(
    PbInsHandle ins, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyInsString(
        ins, buffer, capacity, required_size, PbBackendInsMnemonic);
}

PbStatus PB_CALL pb_ins_get_number_and_size_of_mem_accesses(
    PbInsHandle ins, int32_t* out_num_accesses, int32_t* out_access_size,
    int32_t* out_index_size)
{
    if (!IsValid(ins) || !out_num_accesses || !out_access_size || !out_index_size)
        return PB_ERR_INVALID_ARGUMENT;
    *out_num_accesses = *out_access_size = *out_index_size = 0;
    return GuardExtras([&]() -> PbStatus {
        PbBackendInsGetNumberAndSizeOfMemAccesses(
            ins, out_num_accesses, out_access_size, out_index_size);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_change_reg(
    PbInsHandle ins, PbRegId old_reg, PbRegId new_reg, uint8_t as_read,
    uint8_t* out_changed)
{
    if (!IsValid(ins) || old_reg == PB_REG_INVALID_ ||
        new_reg == PB_REG_INVALID_ || as_read > 1 || !out_changed)
        return PB_ERR_INVALID_ARGUMENT;
    *out_changed = 0;
    if (PbBackendInsInspectionExtrasIsProbeMode())
        return PB_ERR_INVALID_STATE;
    return GuardExtras([&]() -> PbStatus {
        *out_changed = PbBackendInsChangeReg(
            ins, old_reg, new_reg, as_read) ? 1u : 0u;
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_get_far_pointer(
    PbInsHandle ins, uint16_t* out_segment_selector, uint32_t* out_displacement)
{
    if (!IsValid(ins) || !out_segment_selector || !out_displacement)
        return PB_ERR_INVALID_ARGUMENT;
    *out_segment_selector = 0;
    *out_displacement = 0;
    return GuardExtras([&]() -> PbStatus {
        PbBackendInsGetFarPointer(ins, out_segment_selector, out_displacement);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_invalid(PbInsHandle* out_ins)
{
    if (!out_ins)
        return PB_ERR_INVALID_ARGUMENT;
    out_ins->opaque = 0;
    return GuardExtras([&]() -> PbStatus {
        out_ins->opaque = PbBackendInsInvalid();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_xed_dec(
    PbInsHandle ins, PbXedDecodedInstHandle* out_decoded_instruction)
{
    if (!IsValid(ins) || !out_decoded_instruction)
        return PB_ERR_INVALID_ARGUMENT;
    *out_decoded_instruction = 0;
    return GuardExtras([&]() -> PbStatus {
        *out_decoded_instruction = PbBackendInsXedDec(ins);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_xed_exact_map_from_pin_reg(
    PbRegId pin_reg, PbXedRegId* out_xed_reg)
{
    if (!out_xed_reg)
        return PB_ERR_INVALID_ARGUMENT;
    *out_xed_reg = 0;
    return GuardExtras([&]() -> PbStatus {
        *out_xed_reg = PbBackendInsXedExactMapFromPinReg(pin_reg);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_xed_exact_map_to_pin_reg(
    PbXedRegId xed_reg, PbRegId* out_pin_reg)
{
    if (!out_pin_reg)
        return PB_ERR_INVALID_ARGUMENT;
    *out_pin_reg = PB_REG_INVALID_;
    return GuardExtras([&]() -> PbStatus {
        *out_pin_reg = PbBackendInsXedExactMapToPinReg(xed_reg);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_ins_xed_exact_map_to_pin_reg_legacy(
    uint32_t xed_reg, PbRegId* out_pin_reg)
{
    if (!out_pin_reg)
        return PB_ERR_INVALID_ARGUMENT;
    *out_pin_reg = PB_REG_INVALID_;
    return GuardExtras([&]() -> PbStatus {
        *out_pin_reg = PbBackendInsXedExactMapToPinRegLegacy(xed_reg);
        return PB_OK;
    });
}

#define PB_SYNTAX_WRAPPER(name, backend) \
PbStatus PB_CALL name(void) \
{ \
    return GuardExtras([]() -> PbStatus { backend(); return PB_OK; }); \
}

PB_SYNTAX_WRAPPER(pb_pin_set_syntax_att, PbBackendPinSetSyntaxAtt)
PB_SYNTAX_WRAPPER(pb_pin_set_syntax_intel, PbBackendPinSetSyntaxIntel)
PB_SYNTAX_WRAPPER(pb_pin_set_syntax_xed, PbBackendPinSetSyntaxXed)

#undef PB_SYNTAX_WRAPPER
