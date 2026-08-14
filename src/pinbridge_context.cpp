#include "pinbridge/pinbridge.h"

#include "context_backend.h"

namespace
{

bool ValidProcessorState(PbProcessorState state)
{
    return state < PB_PROCESSOR_STATE_COUNT;
}

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

} // namespace

PbStatus PB_CALL pb_pin_get_context_regval(
    PbConstContextHandle context, PbRegId reg, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    if (!context || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() {
        return PbBackendGetContextRegval(context, reg, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_pin_get_full_context_regs_set(PbRegSet* out_regs)
{
    if (!out_regs)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendGetFullContextRegsSet(out_regs); });
}

PbStatus PB_CALL pb_pin_get_context_fpstate(
    PbConstContextHandle context, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    if (!context || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() {
        return PbBackendGetContextFpState(context, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_pin_set_context_fpstate(
    PbContextHandle context, const uint8_t* value, uint64_t value_size)
{
    if (!context || !value)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendSetContextFpState(context, value, value_size); });
}

PbStatus PB_CALL pb_pin_get_context_fxsave(
    PbConstContextHandle context, PbFxSave* out_fxsave)
{
    if (!context || !out_fxsave)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendGetContextFxSave(context, out_fxsave); });
}

PbStatus PB_CALL pb_pin_set_context_fxsave(
    PbContextHandle context, const PbFxSave* fxsave)
{
    if (!context || !fxsave)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendSetContextFxSave(context, fxsave); });
}

PbStatus PB_CALL pb_pin_supports_processor_state(
    PbProcessorState state, uint8_t* out_supported)
{
    if (!ValidProcessorState(state) || !out_supported)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendSupportsProcessorState(state, out_supported); });
}

PbStatus PB_CALL pb_pin_context_contains_state(
    PbContextHandle context, PbProcessorState state, uint8_t* out_contains)
{
    if (!context || !ValidProcessorState(state) || !out_contains)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() {
        return PbBackendContextContainsState(context, state, out_contains);
    });
}

PbStatus PB_CALL pb_pin_save_context(
    PbConstContextHandle source, PbContextHandle destination)
{
    if (!source || !destination)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendSaveContext(source, destination); });
}

PbStatus PB_CALL pb_pin_set_context_reg(
    PbContextHandle context, PbRegId reg, uint64_t value)
{
    if (!context)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendSetContextReg(context, reg, value); });
}

PbStatus PB_CALL pb_pin_set_context_regval(
    PbContextHandle context, PbRegId reg, const uint8_t* value, uint64_t value_size)
{
    if (!context || !value)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() {
        return PbBackendSetContextRegval(context, reg, value, value_size);
    });
}

PbStatus PB_CALL pb_pin_execute_at(PbConstContextHandle context)
{
    if (!context)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        PbBackendExecuteAt(context);
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    PbBackendExecuteAt(context);
#endif
}
