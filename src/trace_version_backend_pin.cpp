#include "pin.H"

#include "trace_version_backend.h"
#include "reg_mapping_pin.h"

namespace
{

static_assert(CALL_ORDER_FIRST == 100, "Pin CALL_ORDER_FIRST changed");
static_assert(CALL_ORDER_DEFAULT == 200, "Pin CALL_ORDER_DEFAULT changed");
static_assert(CALL_ORDER_LAST == 300, "Pin CALL_ORDER_LAST changed");

BBL ToBbl(PbBblHandle handle)
{
    BBL bbl;
    bbl.q_set(handle.opaque);
    return bbl;
}

INS ToIns(PbInsHandle handle)
{
    INS ins;
    ins.q_set(handle.opaque);
    return ins;
}

} // namespace

PbStatus PbBackendBblSetTargetVersion(PbBblHandle bbl, uint64_t version)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    BBL_SetTargetVersion(ToBbl(bbl), static_cast<ADDRINT>(version));
    return PB_OK;
}

PbStatus PbBackendTraceVersion(PbTraceHandle trace, uint64_t* out_version)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_version = static_cast<uint64_t>(
        TRACE_Version(reinterpret_cast<TRACE>(trace)));
    return PB_OK;
}

PbStatus PbBackendInsInsertVersionCase(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    REG native; if (!PbPinRegFromId(reg, &native)) return PB_ERR_INVALID_ARGUMENT;
    INS_InsertVersionCase(
        ToIns(ins), native, static_cast<INT32>(case_value),
        static_cast<ADDRINT>(version), IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertVersionCaseWithCallOrder(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version,
    PbCallOrder call_order)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    REG native; if (!PbPinRegFromId(reg, &native)) return PB_ERR_INVALID_ARGUMENT;
    INS_InsertVersionCase(
        ToIns(ins), native, static_cast<INT32>(case_value),
        static_cast<ADDRINT>(version), IARG_CALL_ORDER,
        static_cast<CALL_ORDER>(call_order), IARG_END);
    return PB_OK;
}
