#ifndef PINBRIDGE_REGSET_CONVERSION_PIN_H
#define PINBRIDGE_REGSET_CONVERSION_PIN_H

#include "pin.H"

#include "pinbridge/pinbridge.h"
#include "reg_mapping_pin.h"

#include <cstring>

namespace PbPinRegSetConversion
{

static_assert(static_cast<UINT32>(REG_LastInRegset) <= PB_REGSET_MAX_REG_ID,
              "PbRegSet is too small for this Pin SDK");

inline uint64_t Mask(UINT32 reg)
{
    return static_cast<uint64_t>(1) << (reg % 64u);
}

inline uint32_t Word(UINT32 reg) { return reg / 64u; }

inline bool IsPinReg(PbRegId reg)
{
    REG native_reg;
    return PbPinRegFromId(reg, &native_reg) && REG_is_reg(native_reg);
}

inline void FromPin(const REGSET& source, PbRegSet* destination)
{
    std::memset(destination, 0, sizeof(*destination));
    for (UINT32 reg = static_cast<UINT32>(REG_FirstInRegset);
         reg <= static_cast<UINT32>(REG_LastInRegset); ++reg)
    {
        PbRegId public_reg;
        if (PbRegIdFromPinReg(static_cast<REG>(reg), &public_reg) &&
            REGSET_Contains(source, static_cast<REG>(reg)))
            destination->words[Word(public_reg)] |= Mask(public_reg);
    }
}

} // namespace PbPinRegSetConversion

#endif
