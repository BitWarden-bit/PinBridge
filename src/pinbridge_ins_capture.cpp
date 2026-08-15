#include "pinbridge/pinbridge.h"

#include "ins_capture_backend.h"

namespace
{

template< typename Function > PbStatus Guard(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

template< typename Callback >
PbStatus Validate(PbInsHandle ins, Callback callback)
{
    return ins.opaque > 0 && callback ? PB_OK : PB_ERR_INVALID_ARGUMENT;
}

} // namespace

#define PB_INS_CAPTURE_WRAPPER(name, callback_type, backend) \
PbStatus PB_CALL name(PbInsHandle ins, callback_type callback, void* user_data) \
{ \
    const PbStatus valid = Validate(ins, callback); \
    if (valid != PB_OK) return valid; \
    return Guard([&]() { return backend(ins, callback, user_data); }); \
}

PB_INS_CAPTURE_WRAPPER(pb_ins_insert_capture_regs, PbInsCaptureRegsCallback,
                       PbBackendInsInsertCaptureRegs)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_capture_regs_ctx,
                       PbInsContextCaptureRegsCallback,
                       PbBackendInsInsertCaptureRegsCtx)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_memory_operands, PbInsMemoryOperandCallback,
                       PbBackendInsInsertMemoryOperands)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_exec, PbInsExecCallback,
                       PbBackendInsInsertExec)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_branch_edge, PbInsBranchEdgeCallback,
                       PbBackendInsInsertBranchEdge)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_capture_exec_bytes, PbInsExecBytesCallback,
                       PbBackendInsInsertCaptureExecBytes)
PB_INS_CAPTURE_WRAPPER(pb_ins_insert_memory_operands_values, PbInsMemoryOperandValueCallback,
                       PbBackendInsInsertMemoryOperandsValues)

#undef PB_INS_CAPTURE_WRAPPER
