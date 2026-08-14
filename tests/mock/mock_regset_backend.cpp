#include "regset_backend.h"

#include <stdint.h>
#include <cstring>
#include <string>

namespace
{

uint64_t Mask(PbRegId reg) { return static_cast<uint64_t>(1) << (reg % 64u); }
uint32_t Word(PbRegId reg) { return reg / 64u; }

uint32_t CountBits(uint64_t value)
{
    uint32_t count = 0;
    while (value != 0)
    {
        value &= value - 1;
        ++count;
    }
    return count;
}

} // namespace

PbStatus PbBackendRegSetAddAll(PbRegSet* set)
{
    for (uint32_t index = 0; index < PB_REGSET_WORD_COUNT; ++index)
        set->words[index] = UINT64_MAX;
    return PB_OK;
}

PbStatus PbBackendRegSetClear(PbRegSet* set)
{
    for (uint32_t index = 0; index < PB_REGSET_WORD_COUNT; ++index)
        set->words[index] = 0;
    return PB_OK;
}

PbStatus PbBackendRegSetContains(const PbRegSet* set, PbRegId reg, uint8_t* out_contains)
{
    *out_contains = (set->words[Word(reg)] & Mask(reg)) != 0 ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendRegSetInsert(PbRegSet* set, PbRegId reg)
{
    set->words[Word(reg)] |= Mask(reg);
    return PB_OK;
}

PbStatus PbBackendRegSetPopCount(const PbRegSet* set, uint32_t* out_count)
{
    uint32_t count = 0;
    for (uint32_t index = 0; index < PB_REGSET_WORD_COUNT; ++index)
        count += CountBits(set->words[index]);
    *out_count = count;
    return PB_OK;
}

PbStatus PbBackendRegSetIsEmpty(const PbRegSet* set, uint8_t* out_is_empty)
{
    for (uint32_t index = 0; index < PB_REGSET_WORD_COUNT; ++index)
    {
        if (set->words[index] != 0)
        {
            *out_is_empty = 0;
            return PB_OK;
        }
    }
    *out_is_empty = 1;
    return PB_OK;
}

PbStatus PbBackendRegSetPopNext(PbRegSet* set, PbRegId* out_reg)
{
    for (PbRegId reg = 0; reg <= PB_REGSET_MAX_REG_ID; ++reg)
    {
        if ((set->words[Word(reg)] & Mask(reg)) != 0)
        {
            set->words[Word(reg)] &= ~Mask(reg);
            *out_reg = reg;
            return PB_OK;
        }
    }
    *out_reg = UINT32_MAX;
    return PB_OK;
}

PbStatus PbBackendRegSetRemove(PbRegSet* set, PbRegId reg)
{
    set->words[Word(reg)] &= ~Mask(reg);
    return PB_OK;
}

PbStatus PbBackendRegSetFirst(PbRegId* out_reg)
{
    *out_reg = 0;
    return PB_OK;
}

PbStatus PbBackendRegSetLast(PbRegId* out_reg)
{
    *out_reg = PB_REGSET_MAX_REG_ID;
    return PB_OK;
}

PbStatus PbBackendRegSetStringShort(
    const PbRegSet* set, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    std::string value;
    for (PbRegId reg = 0; reg <= PB_REGSET_MAX_REG_ID; ++reg)
    {
        if ((set->words[Word(reg)] & Mask(reg)) == 0)
            continue;
        if (!value.empty())
            value += ' ';
        value += std::to_string(reg);
    }
    *required_size = static_cast<uint64_t>(value.size()) + 1;
    if (capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}
