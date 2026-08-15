#include "pin.H"

#include "regset_backend.h"
#include "regset_conversion_pin.h"
#include "reg_mapping_pin.h"

using namespace PbPinRegSetConversion;

namespace
{

bool ToPin(const PbRegSet& source, REGSET* destination)
{
    REGSET_Clear(*destination);
    for (UINT32 reg = 0; reg <= PB_REGSET_MAX_REG_ID; ++reg)
    {
        if ((source.words[Word(reg)] & Mask(reg)) == 0)
            continue;
        if (!IsPinReg(reg))
            return false;
        REG native_reg;
        if (!PbPinRegFromId(reg, &native_reg)) return false;
        REGSET_Insert(*destination, native_reg);
    }
    return true;
}

} // namespace

PbStatus PbBackendRegSetAddAll(PbRegSet* set)
{
    REGSET direct;
    REGSET_AddAll(direct);
    FromPin(direct, set);
    return PB_OK;
}

PbStatus PbBackendRegSetClear(PbRegSet* set)
{
    REGSET direct;
    REGSET_Clear(direct);
    FromPin(direct, set);
    return PB_OK;
}

PbStatus PbBackendRegSetContains(const PbRegSet* set, PbRegId reg, uint8_t* out_contains)
{
    REGSET direct;
    if (!IsPinReg(reg) || !ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg)) return PB_ERR_INVALID_ARGUMENT;
    *out_contains = REGSET_Contains(direct, native_reg) != 0 ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendRegSetInsert(PbRegSet* set, PbRegId reg)
{
    REGSET direct;
    if (!IsPinReg(reg) || !ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg)) return PB_ERR_INVALID_ARGUMENT;
    REGSET_Insert(direct, native_reg);
    FromPin(direct, set);
    return PB_OK;
}

PbStatus PbBackendRegSetPopCount(const PbRegSet* set, uint32_t* out_count)
{
    REGSET direct;
    if (!ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    *out_count = REGSET_PopCount(direct);
    return PB_OK;
}

PbStatus PbBackendRegSetIsEmpty(const PbRegSet* set, uint8_t* out_is_empty)
{
    REGSET direct;
    if (!ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    *out_is_empty = REGSET_PopCountIsZero(direct) != 0 ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendRegSetPopNext(PbRegSet* set, PbRegId* out_reg)
{
    REGSET direct;
    if (!ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    if (!PbRegIdFromPinReg(REGSET_PopNext(direct), out_reg))
        return PB_ERR_INVALID_ARGUMENT;
    FromPin(direct, set);
    return PB_OK;
}

PbStatus PbBackendRegSetRemove(PbRegSet* set, PbRegId reg)
{
    REGSET direct;
    if (!IsPinReg(reg) || !ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg)) return PB_ERR_INVALID_ARGUMENT;
    REGSET_Remove(direct, native_reg);
    FromPin(direct, set);
    return PB_OK;
}

PbStatus PbBackendRegSetFirst(PbRegId* out_reg)
{
    if (!PbRegIdFromPinReg(REG_FirstInRegset, out_reg)) return PB_ERR_INVALID_ARGUMENT;
    return PB_OK;
}

PbStatus PbBackendRegSetLast(PbRegId* out_reg)
{
    if (!PbRegIdFromPinReg(REG_LastInRegset, out_reg)) return PB_ERR_INVALID_ARGUMENT;
    return PB_OK;
}

PbStatus PbBackendRegSetStringShort(
    const PbRegSet* set, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    REGSET direct;
    if (!ToPin(*set, &direct))
        return PB_ERR_INVALID_ARGUMENT;
    const std::string value = REGSET_StringShort(direct);
    *required_size = static_cast<uint64_t>(value.size()) + 1;
    if (capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}
