#include "trace_version_backend.h"

PbStatus PbBackendBblSetTargetVersion(PbBblHandle bbl, uint64_t version)
{
    return bbl.opaque == 1 && version == UINT64_C(7) ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendTraceVersion(PbTraceHandle, uint64_t* out_version)
{
    *out_version = UINT64_C(11);
    return PB_OK;
}

PbStatus PbBackendInsInsertVersionCase(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version)
{
    return ins.opaque == 2 && reg == PB_REG_INST_G0 && case_value == -5 &&
        version == UINT64_C(13) ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendInsInsertVersionCaseWithCallOrder(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version,
    PbCallOrder call_order)
{
    return ins.opaque == 2 && reg == PB_REG_INST_G0 && case_value == 5 &&
        version == UINT64_C(17) && call_order == PB_CALL_ORDER_FIRST + 5 ?
        PB_OK : PB_ERR_INTERNAL;
}
