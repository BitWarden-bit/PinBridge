#include "pinbridge/pinbridge.h"

#include "reg_function_backend.h"

namespace
{

template< typename Function > PbStatus Guard(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        function();
        return PB_OK;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

PbStatus ValidateStep(PbRegId* reg, PbRegId* out_result, bool increment)
{
    if (!reg || !out_result || reg == out_result)
        return PB_ERR_INVALID_ARGUMENT;
    if ((increment && *reg >= PB_REG_LAST) || (!increment && *reg <= PB_REG_INVALID_))
        return PB_ERR_INVALID_ARGUMENT;
    return PB_OK;
}

} // namespace

PbStatus PB_CALL pb_pin_claim_tool_register(PbRegId* out_reg)
{
    if (!out_reg)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { *out_reg = PbBackendClaimToolRegister(); });
}

PbStatus PB_CALL pb_reg_convert_x87_abridged_tag_to_full(
    const PbFxSave* fxsave, uint16_t* out_full_tag)
{
    if (!fxsave || !out_full_tag)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        *out_full_tag = PbBackendConvertX87AbridgedTagToFull(fxsave->bytes);
    });
}

PbStatus PB_CALL pb_reg_string_short(
    PbRegId reg, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!required_size || reg >= PB_REG_LAST)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        const uint64_t required = PbBackendRegStringShort(reg, 0, 0);
        *required_size = required;
        if (!buffer || capacity < required)
            return;
        PbBackendRegStringShort(reg, buffer, capacity);
    }) != PB_OK
        ? PB_ERR_INTERNAL
        : ((!buffer || capacity < *required_size) ? PB_ERR_BUFFER_TOO_SMALL : PB_OK);
}

PbStatus PB_CALL pb_reg_prefix_increment(PbRegId* reg, PbRegId* out_result)
{
    const PbStatus status = ValidateStep(reg, out_result, true);
    if (status != PB_OK)
        return status;
    ++*reg;
    *out_result = *reg;
    return PB_OK;
}

PbStatus PB_CALL pb_reg_postfix_increment(PbRegId* reg, PbRegId* out_previous)
{
    const PbStatus status = ValidateStep(reg, out_previous, true);
    if (status != PB_OK)
        return status;
    *out_previous = *reg;
    ++*reg;
    return PB_OK;
}

PbStatus PB_CALL pb_reg_postfix_decrement(PbRegId* reg, PbRegId* out_previous)
{
    const PbStatus status = ValidateStep(reg, out_previous, false);
    if (status != PB_OK)
        return status;
    *out_previous = *reg;
    --*reg;
    return PB_OK;
}
