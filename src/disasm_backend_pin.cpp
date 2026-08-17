#include "pin.H"

#include "disasm_backend.h"

#include "xed-interface.h"

#include <cstring>

namespace
{

uint32_t Classify(const xed_decoded_inst_t* xedd)
{
    switch (xed_decoded_inst_get_category(xedd))
    {
    case XED_CATEGORY_CALL:
        return 2;
    case XED_CATEGORY_RET:
        return 3;
    case XED_CATEGORY_COND_BR:
    case XED_CATEGORY_UNCOND_BR:
        return 1;
    default:
        return 0;
    }
}

} // namespace

namespace
{

void SetDecodeMode(xed_decoded_inst_t* xedd)
{
#if defined(TARGET_IA32E)
    xed_decoded_inst_set_mode(xedd, XED_MACHINE_MODE_LONG_64, XED_ADDRESS_WIDTH_64b);
#else
    xed_decoded_inst_set_mode(xedd, XED_MACHINE_MODE_LEGACY_32, XED_ADDRESS_WIDTH_32b);
#endif
}

int32_t FlowRegToPb(xed_reg_enum_t reg)
{
    switch (reg)
    {
#if defined(TARGET_IA32E)
    case XED_REG_RAX: return (int32_t)PB_REG_RAX;
    case XED_REG_RCX: return (int32_t)PB_REG_RCX;
    case XED_REG_RDX: return (int32_t)PB_REG_RDX;
    case XED_REG_RBX: return (int32_t)PB_REG_RBX;
    case XED_REG_RSP: return (int32_t)PB_REG_RSP;
    case XED_REG_RBP: return (int32_t)PB_REG_RBP;
    case XED_REG_RSI: return (int32_t)PB_REG_RSI;
    case XED_REG_RDI: return (int32_t)PB_REG_RDI;
    case XED_REG_R8: return (int32_t)PB_REG_R8;
    case XED_REG_R9: return (int32_t)PB_REG_R9;
    case XED_REG_R10: return (int32_t)PB_REG_R10;
    case XED_REG_R11: return (int32_t)PB_REG_R11;
    case XED_REG_R12: return (int32_t)PB_REG_R12;
    case XED_REG_R13: return (int32_t)PB_REG_R13;
    case XED_REG_R14: return (int32_t)PB_REG_R14;
    case XED_REG_R15: return (int32_t)PB_REG_R15;
    case XED_REG_RIP: return (int32_t)PB_REG_RIP;
#else
    case XED_REG_EAX: return (int32_t)PB_REG_EAX;
    case XED_REG_ECX: return (int32_t)PB_REG_ECX;
    case XED_REG_EDX: return (int32_t)PB_REG_EDX;
    case XED_REG_EBX: return (int32_t)PB_REG_EBX;
    case XED_REG_ESP: return (int32_t)PB_REG_ESP;
    case XED_REG_EBP: return (int32_t)PB_REG_EBP;
    case XED_REG_ESI: return (int32_t)PB_REG_ESI;
    case XED_REG_EDI: return (int32_t)PB_REG_EDI;
    case XED_REG_EIP: return (int32_t)PB_REG_EIP;
#endif
    default: return -1;
    }
}

} // namespace

PbStatus PbBackendDisassembleFlow(
    const uint8_t* bytes, uint64_t size, uint64_t address, PbFlowInsn* out)
{
    xed_decoded_inst_t xedd;
    xed_decoded_inst_zero(&xedd);
    SetDecodeMode(&xedd);
    if (xed_decode(&xedd, bytes, static_cast<unsigned int>(size)) != XED_ERROR_NONE)
        return PB_ERR_INVALID_ARGUMENT;
    out->address = address;
    out->size = xed_decoded_inst_get_length(&xedd);
    out->kind = Classify(&xedd);
    out->conditional = 0;
    out->has_target = 0;
    out->ind_reg = 0;
    out->ind_mem = 0;
    out->base_reg = -1;
    out->index_reg = -1;
    out->scale = 0;
    out->disp = 0;
    out->target = 0;
    if (out->size == 0)
        return PB_ERR_INVALID_ARGUMENT;
    const xed_category_enum_t category = xed_decoded_inst_get_category(&xedd);
    if (category == XED_CATEGORY_COND_BR)
        out->conditional = 1;
    if (xed_decoded_inst_get_branch_displacement_width(&xedd) != 0)
    {
        // direct branch/call: target = next-instruction address + displacement
        out->has_target = 1;
        const uint64_t target = address + out->size +
            static_cast<int64_t>(xed_decoded_inst_get_branch_displacement(&xedd));
#if defined(TARGET_IA32E)
        out->target = target;
#else
        // Near IA-32 control flow wraps EIP at 32 bits.
        out->target = static_cast<uint32_t>(target);
#endif
        return PB_OK;
    }
    if (out->kind == 1 || out->kind == 2) // indirect branch / call
    {
        const int32_t reg0 = FlowRegToPb(xed_decoded_inst_get_reg(&xedd, XED_OPERAND_REG0));
        if (reg0 >= 0)
        {
            out->ind_reg = 1;
            out->base_reg = reg0;
            return PB_OK;
        }
        const int32_t base = FlowRegToPb(xed_decoded_inst_get_base_reg(&xedd, 0));
        if (base >= 0)
        {
            out->ind_mem = 1;
            out->base_reg = base;
            out->index_reg = FlowRegToPb(xed_decoded_inst_get_index_reg(&xedd, 0));
            out->scale = xed_decoded_inst_get_scale(&xedd, 0);
            out->disp = static_cast<int64_t>(
                xed_decoded_inst_get_memory_displacement(&xedd, 0));
        }
    }
    return PB_OK;
}

PbStatus PbBackendDisassemble(
    const uint8_t* bytes, uint64_t size, uint64_t address,
    PbDisasmInsn* out, uint64_t capacity, uint64_t* out_count)
{
    uint64_t offset = 0;
    uint64_t count = 0;
    while (offset < size && count < capacity)
    {
        xed_decoded_inst_t xedd;
        xed_decoded_inst_zero(&xedd);
        SetDecodeMode(&xedd);
        const xed_error_enum_t error = xed_decode(
            &xedd, bytes + offset, static_cast<unsigned int>(size - offset));
        if (error != XED_ERROR_NONE)
            break;
        PbDisasmInsn& insn = out[count];
        insn.address = address + offset;
        insn.size = xed_decoded_inst_get_length(&xedd);
        insn.kind = Classify(&xedd);
        insn.text[0] = '\0';
        xed_format_context(
            XED_SYNTAX_INTEL, &xedd, insn.text, sizeof(insn.text) - 1,
            insn.address, 0, 0);
        insn.text[sizeof(insn.text) - 1] = '\0';
        if (insn.size == 0)
            break;
        offset += insn.size;
        ++count;
    }
    *out_count = count;
    return PB_OK;
}
