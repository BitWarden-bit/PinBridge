#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_capture_regs_calls;
static uint32_t g_hook_monitor_calls;
static uint32_t g_capture_regs_ctx_calls;
static uint32_t g_memory_operand_calls;
static uint32_t g_exec_calls;
static uint32_t g_branch_edge_calls;
static uint32_t g_exec_bytes_calls;
static uint32_t g_memory_value_calls;
static uint32_t g_memory_translation_calls;
static uint64_t g_last_address;
static uint64_t g_last_memory_address;
static uint32_t g_last_size;
static uint32_t g_last_access;
static uint64_t g_last_target;
static uint64_t g_last_taken;
static uint64_t g_last_bytes_lo;
static uint64_t g_last_bytes_hi;
static uint64_t g_last_value;

static void PB_CALL OnCaptureRegs(
    uint64_t address, uint32_t thread_id,
    uint64_t rcx, uint64_t rdx, uint64_t r8, uint64_t r9, void* user_data)
{
    if (user_data == &g_capture_regs_calls && thread_id != 0 &&
        rcx != rdx && r8 != r9)
    {
        ++g_capture_regs_calls;
        g_last_address = address;
    }
}

static void PB_CALL OnHookMonitor(
    uint64_t address, uint32_t thread_id,
    uint64_t arg0, uint64_t arg1, uint64_t arg2, uint64_t arg3,
    uint64_t stack_pointer, uint64_t return_value, void* user_data)
{
    if (user_data == &g_hook_monitor_calls && thread_id != 0 &&
        arg0 != arg1 && arg2 != arg3 && stack_pointer != return_value)
    {
        ++g_hook_monitor_calls;
        g_last_address = address;
    }
}

static void PB_CALL OnCaptureRegsCtx(
    uint64_t address, uint32_t thread_id, PbContextHandle context,
    uint64_t rcx, uint64_t rdx, uint64_t r8, uint64_t r9, void* user_data)
{
    if (user_data == &g_capture_regs_ctx_calls && thread_id != 0 && context != 0 &&
        rcx != rdx && r8 != r9)
    {
        ++g_capture_regs_ctx_calls;
        g_last_address = address;
    }
}

static void PB_CALL OnMemoryOperand(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t access, void* user_data)
{
    if (user_data == &g_memory_operand_calls && thread_id != 0)
    {
        ++g_memory_operand_calls;
        g_last_address = instruction_address;
        g_last_memory_address = memory_address;
        g_last_size = size;
        g_last_access = access;
    }
}

static uint64_t PB_CALL OnMemoryTranslate(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t operation, void* user_data)
{
    if (user_data == &g_memory_translation_calls && thread_id != 0 &&
        operation == PB_PIN_MEMOP_LOAD)
    {
        ++g_memory_translation_calls;
        g_last_address = instruction_address;
        g_last_memory_address = memory_address;
        g_last_size = size;
    }
    return memory_address + 0x10;
}

static void PB_CALL OnExec(
    uint64_t address, uint32_t thread_id, uint32_t size, void* user_data)
{
    if (user_data == &g_exec_calls && thread_id != 0)
    {
        ++g_exec_calls;
        g_last_address = address;
        g_last_size = size;
    }
}

static void PB_CALL OnBranchEdge(
    uint64_t address, uint32_t thread_id,
    uint64_t target_address, uint64_t taken, void* user_data)
{
    if (user_data == &g_branch_edge_calls && thread_id != 0)
    {
        ++g_branch_edge_calls;
        g_last_address = address;
        g_last_target = target_address;
        g_last_taken = taken;
    }
}

static void PB_CALL OnExecBytes(
    uint64_t address, uint32_t thread_id, uint32_t size,
    uint64_t bytes_lo, uint64_t bytes_hi, void* user_data)
{
    if (user_data == &g_exec_bytes_calls && thread_id != 0)
    {
        ++g_exec_bytes_calls;
        g_last_address = address;
        g_last_size = size;
        g_last_bytes_lo = bytes_lo;
        g_last_bytes_hi = bytes_hi;
    }
}

static void PB_CALL OnMemoryOperandValue(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t access,
    uint64_t value, void* user_data)
{
    if (user_data == &g_memory_value_calls && thread_id != 0)
    {
        ++g_memory_value_calls;
        g_last_address = instruction_address;
        g_last_memory_address = memory_address;
        g_last_size = size;
        g_last_access = access;
        g_last_value = value;
    }
}

int main(void)
{
    PbInsHandle ins = {7};

    if (pb_ins_insert_capture_regs(ins, OnCaptureRegs, &g_capture_regs_calls) != PB_OK ||
        pb_ins_insert_hook_monitor(ins, OnHookMonitor, &g_hook_monitor_calls) != PB_OK ||
        pb_ins_insert_capture_regs_ctx(ins, OnCaptureRegsCtx, &g_capture_regs_ctx_calls) != PB_OK ||
        pb_ins_insert_memory_operands(ins, OnMemoryOperand, &g_memory_operand_calls) != PB_OK ||
        pb_ins_insert_memory_address_translation(
            ins, OnMemoryTranslate, &g_memory_translation_calls,
            PB_REG_RAX, PB_REG_RBX) != PB_OK ||
        pb_ins_insert_exec(ins, OnExec, &g_exec_calls) != PB_OK ||
        pb_ins_insert_branch_edge(ins, OnBranchEdge, &g_branch_edge_calls) != PB_OK ||
        pb_ins_insert_capture_exec_bytes(ins, OnExecBytes, &g_exec_bytes_calls) != PB_OK ||
        pb_ins_insert_memory_operands_values(ins, OnMemoryOperandValue, &g_memory_value_calls) != PB_OK)
        return 1;
    if (g_capture_regs_calls != 1 || g_hook_monitor_calls != 1 ||
        g_capture_regs_ctx_calls != 1 ||
        g_memory_operand_calls != 1 || g_memory_translation_calls != 1 ||
        g_exec_calls != 1 || g_branch_edge_calls != 1 ||
        g_exec_bytes_calls != 1 || g_memory_value_calls != 1)
        return 2;
    if (g_last_address != 0x1000 || g_last_memory_address != 0x2000 ||
        g_last_access != PB_MEMORY_TYPE_READ ||
        g_last_target != 0x3000 || g_last_taken != 1 ||
        g_last_bytes_lo != 0x11 || g_last_bytes_hi != 0x22 ||
        g_last_value != 0x1234)
        return 3;

    ins.opaque = 0;
    if (pb_ins_insert_capture_regs(ins, OnCaptureRegs, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_hook_monitor(ins, OnHookMonitor, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_capture_regs_ctx(ins, OnCaptureRegsCtx, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_operands(ins, OnMemoryOperand, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_address_translation(
            ins, OnMemoryTranslate, 0, PB_REG_RAX, PB_REG_RBX) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_exec(ins, OnExec, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_branch_edge(ins, OnBranchEdge, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_capture_exec_bytes(ins, OnExecBytes, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_operands_values(ins, OnMemoryOperandValue, 0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    ins.opaque = 1;
    if (pb_ins_insert_capture_regs(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_hook_monitor(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_capture_regs_ctx(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_operands(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_address_translation(
            ins, 0, 0, PB_REG_RAX, PB_REG_RBX) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_address_translation(
            ins, OnMemoryTranslate, 0, PB_REG_RAX, PB_REG_RAX) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_exec(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_branch_edge(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_capture_exec_bytes(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_memory_operands_values(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    if (g_capture_regs_calls != 1 || g_hook_monitor_calls != 1 ||
        g_capture_regs_ctx_calls != 1 ||
        g_memory_operand_calls != 1 || g_memory_translation_calls != 1 ||
        g_exec_calls != 1 || g_branch_edge_calls != 1 ||
        g_exec_bytes_calls != 1 || g_memory_value_calls != 1)
        return 6;
    return 0;
}
