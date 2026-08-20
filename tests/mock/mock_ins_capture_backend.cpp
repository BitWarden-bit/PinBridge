#include "ins_capture_backend.h"

namespace
{
uint32_t g_capture_regs_calls;
uint32_t g_hook_monitor_calls;
uint32_t g_memory_operands_calls;
uint32_t g_exec_calls;
uint32_t g_branch_edge_calls;
uint32_t g_exec_bytes_calls;
uint32_t g_memory_values_calls;
uint32_t g_memory_translation_calls;
}

PbStatus PbBackendInsInsertCaptureRegs(
    PbInsHandle ins, PbInsCaptureRegsCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_capture_regs_calls;
    callback(0x1000, 1, 0x11, 0x22, 0x33, 0x44, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertHookMonitor(
    PbInsHandle ins, PbInsHookMonitorCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_hook_monitor_calls;
    callback(0x1000, 1, 0x11, 0x22, 0x33, 0x44, 0x5000, 0x66, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertCaptureRegsCtx(
    PbInsHandle ins, PbInsContextCaptureRegsCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_capture_regs_calls;
    callback(0x1000, 1, reinterpret_cast<PbContextHandle>(0x1234),
             0x11, 0x22, 0x33, 0x44, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryOperands(
    PbInsHandle ins, PbInsMemoryOperandCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_memory_operands_calls;
    callback(0x1000, 1, 0x2000, 8, PB_MEMORY_TYPE_READ, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryAddressTranslation(
    PbInsHandle ins, PbInsMemoryTranslateCallback callback, void* user_data,
    PbRegId scratch_reg0, PbRegId scratch_reg1)
{
    if (ins.opaque == 0 || !callback || scratch_reg0 == scratch_reg1)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_memory_translation_calls;
    callback(0x1000, 1, 0x2000, 8, PB_PIN_MEMOP_LOAD, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertExec(
    PbInsHandle ins, PbInsExecCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_exec_calls;
    callback(0x1000, 1, 5, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertBranchEdge(
    PbInsHandle ins, PbInsBranchEdgeCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_branch_edge_calls;
    callback(0x1000, 1, 0x3000, 1, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertCaptureExecBytes(
    PbInsHandle ins, PbInsExecBytesCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_exec_bytes_calls;
    callback(0x1000, 1, 5, 0x11, 0x22, user_data);
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryOperandsValues(
    PbInsHandle ins, PbInsMemoryOperandValueCallback callback, void* user_data)
{
    if (ins.opaque == 0 || !callback)
        return PB_ERR_INVALID_ARGUMENT;
    ++g_memory_values_calls;
    callback(0x1000, 1, 0x2000, 8, PB_MEMORY_TYPE_READ, 0x1234, user_data);
    return PB_OK;
}
