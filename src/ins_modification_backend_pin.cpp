#include "pin.H"

#include "ins_modification_backend.h"
#include "reg_mapping_pin.h"

namespace
{

static_assert(IPOINT_INVALID == 0, "Pin 3.31 IPOINT_INVALID changed");
static_assert(IPOINT_BEFORE == 1, "Pin 3.31 IPOINT_BEFORE changed");
static_assert(IPOINT_AFTER == 2, "Pin 3.31 IPOINT_AFTER changed");
static_assert(IPOINT_ANYWHERE == 3, "Pin 3.31 IPOINT_ANYWHERE changed");

INS ToIns(PbInsHandle handle)
{
    INS ins;
    ins.q_set(handle.opaque);
    return ins;
}

IPOINT ToIpoint(PbIpoint ipoint)
{
    return static_cast<IPOINT>(ipoint);
}

} // namespace

PbStatus PbBackendInsDelete(PbInsHandle ins)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    INS_Delete(ToIns(ins));
    return PB_OK;
}

PbStatus PbBackendInsInsertDirectJump(
    PbInsHandle ins, PbIpoint ipoint, uint64_t target)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    INS_InsertDirectJump(ToIns(ins), ToIpoint(ipoint), static_cast<ADDRINT>(target));
    return PB_OK;
}

PbStatus PbBackendInsInsertIndirectJump(
    PbInsHandle ins, PbIpoint ipoint, PbRegId reg)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    REG native;
    if (!PbPinRegFromId(reg, &native)) return PB_ERR_INVALID_ARGUMENT;
    INS_InsertIndirectJump(ToIns(ins), ToIpoint(ipoint), native);
    return PB_OK;
}

PbStatus PbBackendInsRewriteMemoryOperand(
    PbInsHandle ins, uint32_t memindex, PbRegId reg)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const INS direct = ToIns(ins);
    if (memindex >= INS_MemoryOperandCount(direct))
        return PB_ERR_INVALID_ARGUMENT;
    if (INS_HasScatteredMemoryAccess(direct))
        return PB_ERR_UNSUPPORTED;
    REG native;
    if (!PbPinRegFromId(reg, &native)) return PB_ERR_INVALID_ARGUMENT;
    INS_RewriteMemoryOperand(direct, memindex, native);
    return PB_OK;
}

PbStatus PbBackendInsRewriteScatteredMemoryOperand(
    PbInsHandle ins, uint32_t memindex)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const INS direct = ToIns(ins);
    if (memindex >= INS_MemoryOperandCount(direct))
        return PB_ERR_INVALID_ARGUMENT;
    if (!INS_HasScatteredMemoryAccess(direct))
        return PB_ERR_UNSUPPORTED;
    INS_RewriteScatteredMemoryOperand(direct, memindex);
    return PB_OK;
}
