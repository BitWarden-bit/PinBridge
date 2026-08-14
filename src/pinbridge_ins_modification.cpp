#include "pinbridge/pinbridge.h"

#include "ins_modification_backend.h"

namespace
{

template< typename Function > PbStatus Guard(Function function)
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

bool IsValidReg(PbRegId reg)
{
    return reg > PB_REG_NONE && reg < PB_REG_LAST;
}

bool IsJumpIpoint(PbIpoint ipoint)
{
    return ipoint == PB_IPOINT_BEFORE || ipoint == PB_IPOINT_AFTER;
}

} // namespace

PbStatus PB_CALL pb_ins_delete(PbInsHandle ins)
{
    if (ins.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendInsDelete(ins); });
}

PbStatus PB_CALL pb_ins_insert_direct_jump(
    PbInsHandle ins, PbIpoint ipoint, uint64_t target)
{
    if (ins.opaque <= 0 || !IsJumpIpoint(ipoint))
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        return PbBackendInsInsertDirectJump(ins, ipoint, target);
    });
}

PbStatus PB_CALL pb_ins_insert_indirect_jump(
    PbInsHandle ins, PbIpoint ipoint, PbRegId reg)
{
    if (ins.opaque <= 0 || !IsJumpIpoint(ipoint) || !IsValidReg(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        return PbBackendInsInsertIndirectJump(ins, ipoint, reg);
    });
}

PbStatus PB_CALL pb_ins_rewrite_memory_operand(
    PbInsHandle ins, uint32_t memindex, PbRegId reg)
{
    if (ins.opaque <= 0 || !IsValidReg(reg))
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        return PbBackendInsRewriteMemoryOperand(ins, memindex, reg);
    });
}

PbStatus PB_CALL pb_ins_rewrite_scattered_memory_operand(
    PbInsHandle ins, uint32_t memindex)
{
    if (ins.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        return PbBackendInsRewriteScatteredMemoryOperand(ins, memindex);
    });
}
