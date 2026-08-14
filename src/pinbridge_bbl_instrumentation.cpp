#include "pinbridge/pinbridge.h"

#include "bbl_instrumentation_backend.h"

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

PbStatus PB_CALL pb_bbl_insert_call_before(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data)
{
    if (bbl.opaque <= 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendBblInsertCallBefore(bbl, callback, user_data);
    });
}

PbStatus PB_CALL pb_bbl_insert_if_call_before(
    PbBblHandle bbl, PbBblPredicateCallback callback, void* user_data)
{
    if (bbl.opaque <= 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendBblInsertIfCallBefore(bbl, callback, user_data);
    });
}

PbStatus PB_CALL pb_bbl_insert_then_call_before(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data)
{
    if (bbl.opaque <= 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardInsertion([&]() {
        return PbBackendBblInsertThenCallBefore(bbl, callback, user_data);
    });
}
