#include "pinbridge/pinbridge.h"

#include "trace_version_backend.h"

namespace
{

template< typename Function > PbStatus GuardVersionOperation(Function function)
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

bool IsValidVersionRegister(PbRegId reg)
{
    return reg > PB_REG_NONE && reg < PB_REG_LAST;
}

} // namespace

PbStatus PB_CALL pb_bbl_set_target_version(PbBblHandle bbl, uint64_t version)
{
    if (bbl.opaque == 0)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardVersionOperation(
        [&]() { return PbBackendBblSetTargetVersion(bbl, version); });
}

PbStatus PB_CALL pb_trace_version(PbTraceHandle trace, uint64_t* out_version)
{
    if (!trace || !out_version)
        return PB_ERR_INVALID_ARGUMENT;
    *out_version = 0;
    return GuardVersionOperation(
        [&]() { return PbBackendTraceVersion(trace, out_version); });
}

PbStatus PB_CALL pb_ins_insert_version_case(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version)
{
    if (ins.opaque == 0 || !IsValidVersionRegister(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardVersionOperation([&]() {
        return PbBackendInsInsertVersionCase(ins, reg, case_value, version);
    });
}

PbStatus PB_CALL pb_ins_insert_version_case_with_call_order(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version,
    PbCallOrder call_order)
{
    if (ins.opaque == 0 || !IsValidVersionRegister(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardVersionOperation([&]() {
        return PbBackendInsInsertVersionCaseWithCallOrder(
            ins, reg, case_value, version, call_order);
    });
}
