#ifndef PINBRIDGE_REG_MAPPING_PIN_H
#define PINBRIDGE_REG_MAPPING_PIN_H

#include "pin.H"
#include "pinbridge/pinbridge.h"

// PbRegId values are wire IDs from the matched x64 inventory.  Pin's IA32
// REG enum has a different layout after the general-purpose registers, so a
// raw cast can turn PB_REG_EIP (58) into REG_YMM5. Keep this conversion in one
// place and reject registers that do not exist on the running architecture.
inline bool PbPinRegFromId(PbRegId value, REG* out)
{
#if defined(TARGET_IA32E)
    const REG reg = static_cast<REG>(value);
    if (!REG_is_reg(reg))
        return false;
    *out = reg;
    return true;
#else
    switch (value)
    {
    case PB_REG_RDI: *out = REG_GDI; return true;
    case PB_REG_RSI: *out = REG_GSI; return true;
    case PB_REG_RBP: *out = REG_GBP; return true;
    case PB_REG_RSP: *out = REG_STACK_PTR; return true;
    case PB_REG_RBX: *out = REG_GBX; return true;
    case PB_REG_RDX: *out = REG_GDX; return true;
    case PB_REG_RCX: *out = REG_GCX; return true;
    case PB_REG_RAX: *out = REG_GAX; return true;
    case PB_REG_RFLAGS: *out = REG_GFLAGS; return true;
    case PB_REG_RIP: *out = REG_INST_PTR; return true;
    case PB_REG_AL: *out = REG_AL; return true;
    case PB_REG_AH: *out = REG_AH; return true;
    case PB_REG_AX: *out = REG_AX; return true;
    case PB_REG_CL: *out = REG_CL; return true;
    case PB_REG_CH: *out = REG_CH; return true;
    case PB_REG_CX: *out = REG_CX; return true;
    case PB_REG_DL: *out = REG_DL; return true;
    case PB_REG_DH: *out = REG_DH; return true;
    case PB_REG_DX: *out = REG_DX; return true;
    case PB_REG_BL: *out = REG_BL; return true;
    case PB_REG_BH: *out = REG_BH; return true;
    case PB_REG_BX: *out = REG_BX; return true;
    case PB_REG_BP: *out = REG_BP; return true;
    case PB_REG_SI: *out = REG_SI; return true;
    case PB_REG_DI: *out = REG_DI; return true;
    case PB_REG_SP: *out = REG_SP; return true;
    case PB_REG_FLAGS: *out = REG_FLAGS; return true;
    case PB_REG_IP: *out = REG_IP; return true;
    case PB_REG_EDI: *out = REG_EDI; return true;
    case PB_REG_ESI: *out = REG_ESI; return true;
    case PB_REG_EBP: *out = REG_EBP; return true;
    case PB_REG_ESP: *out = REG_ESP; return true;
    case PB_REG_EBX: *out = REG_EBX; return true;
    case PB_REG_EDX: *out = REG_EDX; return true;
    case PB_REG_ECX: *out = REG_ECX; return true;
    case PB_REG_EAX: *out = REG_EAX; return true;
    case PB_REG_EFLAGS: *out = REG_EFLAGS; return true;
    case PB_REG_EIP: *out = REG_EIP; return true;
    case PB_REG_MM0: *out = REG_MM0; return true;
    case PB_REG_MM1: *out = REG_MM1; return true;
    case PB_REG_MM2: *out = REG_MM2; return true;
    case PB_REG_MM3: *out = REG_MM3; return true;
    case PB_REG_MM4: *out = REG_MM4; return true;
    case PB_REG_MM5: *out = REG_MM5; return true;
    case PB_REG_MM6: *out = REG_MM6; return true;
    case PB_REG_MM7: *out = REG_MM7; return true;
    case PB_REG_XMM0: *out = REG_XMM0; return true;
    case PB_REG_XMM1: *out = REG_XMM1; return true;
    case PB_REG_XMM2: *out = REG_XMM2; return true;
    case PB_REG_XMM3: *out = REG_XMM3; return true;
    case PB_REG_XMM4: *out = REG_XMM4; return true;
    case PB_REG_XMM5: *out = REG_XMM5; return true;
    case PB_REG_XMM6: *out = REG_XMM6; return true;
    case PB_REG_XMM7: *out = REG_XMM7; return true;
    case PB_REG_YMM0: *out = REG_YMM0; return true;
    case PB_REG_YMM1: *out = REG_YMM1; return true;
    case PB_REG_YMM2: *out = REG_YMM2; return true;
    case PB_REG_YMM3: *out = REG_YMM3; return true;
    case PB_REG_YMM4: *out = REG_YMM4; return true;
    case PB_REG_YMM5: *out = REG_YMM5; return true;
    case PB_REG_YMM6: *out = REG_YMM6; return true;
    case PB_REG_YMM7: *out = REG_YMM7; return true;
    default: return false;
    }
#endif
}

inline bool PbRegIdFromPinReg(REG reg, PbRegId* out)
{
#if defined(TARGET_IA32E)
    if (!REG_is_reg(reg)) return false;
    *out = static_cast<PbRegId>(reg);
    return true;
#else
    switch (reg)
    {
    case REG_GDI: *out = PB_REG_EDI; return true;
    case REG_GSI: *out = PB_REG_ESI; return true;
    case REG_GBP: *out = PB_REG_EBP; return true;
    case REG_STACK_PTR: *out = PB_REG_ESP; return true;
    case REG_GBX: *out = PB_REG_EBX; return true;
    case REG_GDX: *out = PB_REG_EDX; return true;
    case REG_GCX: *out = PB_REG_ECX; return true;
    case REG_GAX: *out = PB_REG_EAX; return true;
    case REG_GFLAGS: *out = PB_REG_EFLAGS; return true;
    case REG_INST_PTR: *out = PB_REG_EIP; return true;
    case REG_AL: *out = PB_REG_AL; return true;
    case REG_AH: *out = PB_REG_AH; return true;
    case REG_AX: *out = PB_REG_AX; return true;
    case REG_CL: *out = PB_REG_CL; return true;
    case REG_CH: *out = PB_REG_CH; return true;
    case REG_CX: *out = PB_REG_CX; return true;
    case REG_DL: *out = PB_REG_DL; return true;
    case REG_DH: *out = PB_REG_DH; return true;
    case REG_DX: *out = PB_REG_DX; return true;
    case REG_BL: *out = PB_REG_BL; return true;
    case REG_BH: *out = PB_REG_BH; return true;
    case REG_BX: *out = PB_REG_BX; return true;
    case REG_BP: *out = PB_REG_BP; return true;
    case REG_SI: *out = PB_REG_SI; return true;
    case REG_DI: *out = PB_REG_DI; return true;
    case REG_SP: *out = PB_REG_SP; return true;
    case REG_FLAGS: *out = PB_REG_FLAGS; return true;
    case REG_IP: *out = PB_REG_IP; return true;
    case REG_MM0: *out = PB_REG_MM0; return true;
    case REG_MM1: *out = PB_REG_MM1; return true;
    case REG_MM2: *out = PB_REG_MM2; return true;
    case REG_MM3: *out = PB_REG_MM3; return true;
    case REG_MM4: *out = PB_REG_MM4; return true;
    case REG_MM5: *out = PB_REG_MM5; return true;
    case REG_MM6: *out = PB_REG_MM6; return true;
    case REG_MM7: *out = PB_REG_MM7; return true;
    case REG_XMM0: *out = PB_REG_XMM0; return true;
    case REG_XMM1: *out = PB_REG_XMM1; return true;
    case REG_XMM2: *out = PB_REG_XMM2; return true;
    case REG_XMM3: *out = PB_REG_XMM3; return true;
    case REG_XMM4: *out = PB_REG_XMM4; return true;
    case REG_XMM5: *out = PB_REG_XMM5; return true;
    case REG_XMM6: *out = PB_REG_XMM6; return true;
    case REG_XMM7: *out = PB_REG_XMM7; return true;
    case REG_YMM0: *out = PB_REG_YMM0; return true;
    case REG_YMM1: *out = PB_REG_YMM1; return true;
    case REG_YMM2: *out = PB_REG_YMM2; return true;
    case REG_YMM3: *out = PB_REG_YMM3; return true;
    case REG_YMM4: *out = PB_REG_YMM4; return true;
    case REG_YMM5: *out = PB_REG_YMM5; return true;
    case REG_YMM6: *out = PB_REG_YMM6; return true;
    case REG_YMM7: *out = PB_REG_YMM7; return true;
    default: return false;
    }
#endif
}

#endif
