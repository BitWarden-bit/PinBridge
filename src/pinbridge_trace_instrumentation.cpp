#include "pinbridge/pinbridge.h"

#include "trace_instrumentation_backend.h"

namespace
{

template< typename Function > PbStatus GuardInsertion(Function function)
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

PbStatus PB_CALL pb_trace_insert_call_before(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data)
{
    if (!trace || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendTraceInsertCallBefore(trace, callback, user_data);
    });
}

PbStatus PB_CALL pb_trace_insert_if_call_before(
    PbTraceHandle trace, PbTracePredicateCallback callback, void* user_data)
{
    if (!trace || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendTraceInsertIfCallBefore(trace, callback, user_data);
    });
}

PbStatus PB_CALL pb_trace_insert_then_call_before(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data)
{
    if (!trace || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendTraceInsertThenCallBefore(trace, callback, user_data);
    });
}
