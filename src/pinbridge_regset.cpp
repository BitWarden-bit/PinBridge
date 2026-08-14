#include "pinbridge/pinbridge.h"

#include "regset_backend.h"

namespace
{

template< typename Function > PbStatus Invoke(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return function();
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

bool IsRepresentable(PbRegId reg) { return reg <= PB_REGSET_MAX_REG_ID; }

} // namespace

PbStatus PB_CALL pb_regset_add_all(PbRegSet* set)
{
    if (!set)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetAddAll(set); });
}

PbStatus PB_CALL pb_regset_clear(PbRegSet* set)
{
    if (!set)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetClear(set); });
}

PbStatus PB_CALL pb_regset_contains(const PbRegSet* set, PbRegId reg, uint8_t* out_contains)
{
    if (!set || !out_contains || !IsRepresentable(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetContains(set, reg, out_contains); });
}

PbStatus PB_CALL pb_regset_insert(PbRegSet* set, PbRegId reg)
{
    if (!set || !IsRepresentable(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetInsert(set, reg); });
}

PbStatus PB_CALL pb_regset_pop_count(const PbRegSet* set, uint32_t* out_count)
{
    if (!set || !out_count)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetPopCount(set, out_count); });
}

PbStatus PB_CALL pb_regset_is_empty(const PbRegSet* set, uint8_t* out_is_empty)
{
    if (!set || !out_is_empty)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetIsEmpty(set, out_is_empty); });
}

PbStatus PB_CALL pb_regset_pop_next(PbRegSet* set, PbRegId* out_reg)
{
    if (!set || !out_reg)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetPopNext(set, out_reg); });
}

PbStatus PB_CALL pb_regset_remove(PbRegSet* set, PbRegId reg)
{
    if (!set || !IsRepresentable(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetRemove(set, reg); });
}

PbStatus PB_CALL pb_regset_first_reg(PbRegId* out_reg)
{
    if (!out_reg)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetFirst(out_reg); });
}

PbStatus PB_CALL pb_regset_last_reg(PbRegId* out_reg)
{
    if (!out_reg)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendRegSetLast(out_reg); });
}

PbStatus PB_CALL pb_regset_string_short(
    const PbRegSet* set, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!set || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() {
        return PbBackendRegSetStringShort(set, buffer, capacity, required_size);
    });
}
