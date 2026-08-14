#include "ins_modification_backend.h"

namespace
{

bool IsJumpIpoint(PbIpoint ipoint)
{
    return ipoint == PB_IPOINT_BEFORE || ipoint == PB_IPOINT_AFTER;
}

} // namespace

PbStatus PbBackendInsDelete(PbInsHandle ins)
{
    return ins.opaque == 2 ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendInsInsertDirectJump(
    PbInsHandle ins, PbIpoint ipoint, uint64_t target)
{
    return ins.opaque == 2 && IsJumpIpoint(ipoint) && target == UINT64_C(0x1234)
        ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendInsInsertIndirectJump(
    PbInsHandle ins, PbIpoint ipoint, PbRegId reg)
{
    return ins.opaque == 2 && IsJumpIpoint(ipoint) && reg == PB_REG_RAX
        ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendInsRewriteMemoryOperand(
    PbInsHandle ins, uint32_t memindex, PbRegId reg)
{
    return ins.opaque == 2 && memindex < 2 && reg == PB_REG_RAX
        ? PB_OK : (ins.opaque == 3 ? PB_ERR_UNSUPPORTED : PB_ERR_INVALID_ARGUMENT);
}

PbStatus PbBackendInsRewriteScatteredMemoryOperand(
    PbInsHandle ins, uint32_t memindex)
{
    return ins.opaque == 3 && memindex == 0
        ? PB_OK : (ins.opaque == 2 ? PB_ERR_UNSUPPORTED : PB_ERR_INVALID_ARGUMENT);
}
