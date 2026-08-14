#include "pin.H"

#include "control_call_application_backend.h"

namespace
{

const CONTEXT* ToPinContext(PbConstContextHandle context)
{
    return reinterpret_cast<const CONTEXT*>(context);
}

AFUNPTR ToApplicationFunction(uint64_t function_address)
{
    return reinterpret_cast<AFUNPTR>(static_cast<ADDRINT>(function_address));
}

PbStatus RequireJitMode()
{
    return PIN_IsProbeMode() ? PB_ERR_INVALID_STATE : PB_OK;
}

} // namespace

PbStatus PbBackendCallApplicationVoid0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address)
{
    const PbStatus mode = RequireJitMode();
    if (mode != PB_OK)
        return mode;
    PIN_CallApplicationFunction(
        ToPinContext(context), static_cast<THREADID>(thread_id),
        CALLINGSTD_DEFAULT, ToApplicationFunction(function_address), 0,
        PIN_PARG_END());
    return PB_OK;
}

PbStatus PbBackendCallApplicationU640(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t* out_result)
{
    const PbStatus mode = RequireJitMode();
    if (mode != PB_OK)
        return mode;
    unsigned long long result = 0;
    PIN_CallApplicationFunction(
        ToPinContext(context), static_cast<THREADID>(thread_id),
        CALLINGSTD_DEFAULT, ToApplicationFunction(function_address), 0,
        PIN_PARG(unsigned long long), &result, PIN_PARG_END());
    *out_result = static_cast<uint64_t>(result);
    return PB_OK;
}

PbStatus PbBackendCallApplicationU641(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t* out_result)
{
    const PbStatus mode = RequireJitMode();
    if (mode != PB_OK)
        return mode;
    unsigned long long result = 0;
    PIN_CallApplicationFunction(
        ToPinContext(context), static_cast<THREADID>(thread_id),
        CALLINGSTD_DEFAULT, ToApplicationFunction(function_address), 0,
        PIN_PARG(unsigned long long), &result,
        PIN_PARG(unsigned long long), static_cast<unsigned long long>(argument0),
        PIN_PARG_END());
    *out_result = static_cast<uint64_t>(result);
    return PB_OK;
}

PbStatus PbBackendCallApplicationU642(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t argument1,
    uint64_t* out_result)
{
    const PbStatus mode = RequireJitMode();
    if (mode != PB_OK)
        return mode;
    unsigned long long result = 0;
    PIN_CallApplicationFunction(
        ToPinContext(context), static_cast<THREADID>(thread_id),
        CALLINGSTD_DEFAULT, ToApplicationFunction(function_address), 0,
        PIN_PARG(unsigned long long), &result,
        PIN_PARG(unsigned long long), static_cast<unsigned long long>(argument0),
        PIN_PARG(unsigned long long), static_cast<unsigned long long>(argument1),
        PIN_PARG_END());
    *out_result = static_cast<uint64_t>(result);
    return PB_OK;
}

PbStatus PbBackendCallApplicationPtrUsize(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t size, void** out_result)
{
    const PbStatus mode = RequireJitMode();
    if (mode != PB_OK)
        return mode;
    void* result = 0;
    PIN_CallApplicationFunction(
        ToPinContext(context), static_cast<THREADID>(thread_id),
        CALLINGSTD_DEFAULT, ToApplicationFunction(function_address), 0,
        PIN_PARG(void*), &result,
        PIN_PARG(unsigned long long), static_cast<unsigned long long>(size),
        PIN_PARG_END());
    *out_result = result;
    return PB_OK;
}
