#include "pinbridge/pinbridge.h"

#include "control_call_application_backend.h"

namespace
{

bool InvalidCommonArguments(
    PbConstContextHandle context, uint64_t function_address)
{
    return context == 0 || function_address == 0;
}

} // namespace

PbStatus PB_CALL pb_pin_call_application_function_void_0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address)
{
    if (InvalidCommonArguments(context, function_address))
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendCallApplicationVoid0(context, thread_id, function_address);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

PbStatus PB_CALL pb_pin_call_application_function_u64_0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t* out_result)
{
    if (out_result)
        *out_result = 0;
    if (InvalidCommonArguments(context, function_address) || !out_result)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendCallApplicationU640(
            context, thread_id, function_address, out_result);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

PbStatus PB_CALL pb_pin_call_application_function_u64_1(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t* out_result)
{
    if (out_result)
        *out_result = 0;
    if (InvalidCommonArguments(context, function_address) || !out_result)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendCallApplicationU641(
            context, thread_id, function_address, argument0, out_result);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

PbStatus PB_CALL pb_pin_call_application_function_u64_2(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t argument1,
    uint64_t* out_result)
{
    if (out_result)
        *out_result = 0;
    if (InvalidCommonArguments(context, function_address) || !out_result)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendCallApplicationU642(
            context, thread_id, function_address, argument0, argument1, out_result);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

PbStatus PB_CALL pb_pin_call_application_function_ptr_usize(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t size, void** out_result)
{
    if (out_result)
        *out_result = 0;
    if (InvalidCommonArguments(context, function_address) || !out_result)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        return PbBackendCallApplicationPtrUsize(
            context, thread_id, function_address, size, out_result);
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}
