#include "pinbridge/pinbridge.h"

#include "physical_context_backend.h"

namespace
{

bool IsPhysicalIntegerReg(PbRegId reg)
{
    return reg >= PB_REG_PHYSICAL_INTEGER_BASE &&
           reg <= PB_REG_PHYSICAL_INTEGER_END;
}

template< typename Function > PbStatus InvokePhysicalContext(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_get_physical_context_reg(
    PbConstPhysicalContextHandle context, PbRegId reg, uint64_t* out_value)
{
    if (!context || !out_value || !IsPhysicalIntegerReg(reg))
        return PB_ERR_INVALID_ARGUMENT;
    *out_value = 0;
    return InvokePhysicalContext(
        [&]() { return PbBackendGetPhysicalContextReg(context, reg, out_value); });
}

PbStatus PB_CALL pb_pin_set_physical_context_reg(
    PbPhysicalContextHandle context, PbRegId reg, uint64_t value)
{
    if (!context || !IsPhysicalIntegerReg(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return InvokePhysicalContext(
        [&]() { return PbBackendSetPhysicalContextReg(context, reg, value); });
}

PbStatus PB_CALL pb_pin_get_physical_context_fxsave(
    PbConstPhysicalContextHandle context, PbFxSave* out_fxsave)
{
    if (!context || !out_fxsave)
        return PB_ERR_INVALID_ARGUMENT;
    return InvokePhysicalContext(
        [&]() { return PbBackendGetPhysicalContextFxSave(context, out_fxsave); });
}

PbStatus PB_CALL pb_pin_set_physical_context_fxsave(
    PbPhysicalContextHandle context, const PbFxSave* fxsave)
{
    if (!context || !fxsave)
        return PB_ERR_INVALID_ARGUMENT;
    return InvokePhysicalContext(
        [&]() { return PbBackendSetPhysicalContextFxSave(context, fxsave); });
}
