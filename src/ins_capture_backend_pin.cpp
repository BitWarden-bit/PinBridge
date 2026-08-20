#include "pin.H"

#include "ins_capture_backend.h"
#include "persistent_callback_state.h"
#include "reg_mapping_pin.h"

namespace
{

typedef PbPersistentCallbackState<PbInsCaptureRegsCallback> CaptureRegsState;
typedef PbPersistentCallbackState<PbInsHookMonitorCallback> HookMonitorState;
typedef PbPersistentCallbackState<PbInsContextCaptureRegsCallback>
    ContextCaptureRegsState;
typedef PbPersistentCallbackState<PbInsMemoryOperandCallback> MemoryOperandState;
typedef PbPersistentCallbackState<PbInsMemoryTranslateCallback> MemoryTranslateState;
typedef PbPersistentCallbackState<PbInsExecCallback> ExecState;
typedef PbPersistentCallbackState<PbInsBranchEdgeCallback> BranchEdgeState;
typedef PbPersistentCallbackState<PbInsExecBytesCallback> ExecBytesState;
typedef PbPersistentCallbackState<PbInsMemoryOperandValueCallback>
    MemoryOperandValueState;

CaptureRegsState* g_capture_regs_states = 0;
HookMonitorState* g_hook_monitor_states = 0;
ContextCaptureRegsState* g_context_capture_regs_states = 0;
MemoryOperandState* g_memory_operand_states = 0;
MemoryTranslateState* g_memory_translate_states = 0;
ExecState* g_exec_states = 0;
BranchEdgeState* g_branch_edge_states = 0;
ExecBytesState* g_exec_bytes_states = 0;
MemoryOperandValueState* g_memory_operand_value_states = 0;

INS ToIns(PbInsHandle handle)
{
    INS ins;
    ins.q_set(handle.opaque);
    return ins;
}

VOID OnCaptureRegs(
    ADDRINT address, THREADID thread_id,
    ADDRINT rcx, ADDRINT rdx, ADDRINT r8, ADDRINT r9, VOID* raw_state)
{
    CaptureRegsState* state = static_cast<CaptureRegsState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(rcx), static_cast<uint64_t>(rdx),
        static_cast<uint64_t>(r8), static_cast<uint64_t>(r9),
        state->user_data);
}

VOID OnHookMonitor(
    ADDRINT address, THREADID thread_id,
    ADDRINT arg0, ADDRINT arg1, ADDRINT arg2, ADDRINT arg3,
    ADDRINT stack_pointer, ADDRINT return_value, VOID* raw_state)
{
    HookMonitorState* state = static_cast<HookMonitorState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(arg0), static_cast<uint64_t>(arg1),
        static_cast<uint64_t>(arg2), static_cast<uint64_t>(arg3),
        static_cast<uint64_t>(stack_pointer),
        static_cast<uint64_t>(return_value), state->user_data);
}

VOID OnContextCaptureRegs(
    ADDRINT address, THREADID thread_id, CONTEXT* context,
    ADDRINT rcx, ADDRINT rdx, ADDRINT r8, ADDRINT r9, VOID* raw_state)
{
    ContextCaptureRegsState* state =
        static_cast<ContextCaptureRegsState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        reinterpret_cast<PbContextHandle>(context),
        static_cast<uint64_t>(rcx), static_cast<uint64_t>(rdx),
        static_cast<uint64_t>(r8), static_cast<uint64_t>(r9),
        state->user_data);
}

VOID OnMemoryOperand(
    ADDRINT instruction_address, THREADID thread_id,
    ADDRINT memory_address, UINT32 size, UINT32 access, VOID* raw_state)
{
    MemoryOperandState* state = static_cast<MemoryOperandState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(instruction_address),
        static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(memory_address), size, access, state->user_data);
}

ADDRINT OnMemoryTranslate(
    ADDRINT instruction_address, THREADID thread_id,
    ADDRINT memory_address, UINT32 size, UINT32 operation, VOID* raw_state)
{
    MemoryTranslateState* state = static_cast<MemoryTranslateState*>(raw_state);
    return static_cast<ADDRINT>(state->callback(
        static_cast<uint64_t>(instruction_address),
        static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(memory_address), size, operation,
        state->user_data));
}

VOID OnExec(ADDRINT address, THREADID thread_id, UINT32 size, VOID* raw_state)
{
    ExecState* state = static_cast<ExecState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        size, state->user_data);
}

VOID OnBranchEdge(
    ADDRINT address, THREADID thread_id,
    ADDRINT target_address, BOOL taken, VOID* raw_state)
{
    BranchEdgeState* state = static_cast<BranchEdgeState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(target_address),
        static_cast<uint64_t>(taken ? 1u : 0u), state->user_data);
}

// Up to `size` bytes (capped at 8) safe-copied from the application address,
// zero-padded. A failed copy reports zeros -- the capture never faults.
uint64_t SafeReadValue(ADDRINT address, UINT32 size)
{
    uint8_t bytes[8] = {};
    const UINT32 want = size < static_cast<UINT32>(sizeof(bytes))
        ? size : static_cast<UINT32>(sizeof(bytes));
    if (address != 0 && want > 0)
        PIN_SafeCopy(bytes, reinterpret_cast<const VOID*>(address), want);
    uint64_t value = 0;
    for (UINT32 i = 0; i < 8; ++i)
        value |= static_cast<uint64_t>(bytes[i]) << (8 * i);
    return value;
}

VOID OnExecBytes(ADDRINT address, THREADID thread_id, UINT32 size, VOID* raw_state)
{
    ExecBytesState* state = static_cast<ExecBytesState*>(raw_state);
    // x64 instructions are at most 15 bytes; anything past that stays zero.
    uint8_t bytes[15] = {};
    const UINT32 want = size < static_cast<UINT32>(sizeof(bytes))
        ? size : static_cast<UINT32>(sizeof(bytes));
    if (want > 0)
        PIN_SafeCopy(bytes, reinterpret_cast<const VOID*>(address), want);
    uint64_t bytes_lo = 0;
    uint64_t bytes_hi = 0;
    for (UINT32 i = 0; i < 8; ++i)
        bytes_lo |= static_cast<uint64_t>(bytes[i]) << (8 * i);
    for (UINT32 i = 0; i < 7; ++i)
        bytes_hi |= static_cast<uint64_t>(bytes[8 + i]) << (8 * i);
    state->callback(
        static_cast<uint64_t>(address), static_cast<uint32_t>(thread_id),
        size, bytes_lo, bytes_hi, state->user_data);
}

VOID OnMemoryOperandValue(
    ADDRINT instruction_address, THREADID thread_id,
    ADDRINT memory_address, UINT32 size, UINT32 access, VOID* raw_state)
{
    MemoryOperandValueState* state = static_cast<MemoryOperandValueState*>(raw_state);
    state->callback(
        static_cast<uint64_t>(instruction_address),
        static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(memory_address), size, access,
        SafeReadValue(memory_address, size), state->user_data);
}

// IARG_MEMORYOP_EA is only valid at IPOINT_BEFORE, but the value a write
// operand leaves behind only exists in [ea] AFTER the instruction. The
// BEFORE call parks the EA in a per-thread TLS slot (one key per
// write-operand ordinal, the EA stored as the pointer value so the analysis
// callback never allocates); the AFTER call reads it back and safe-copies
// the content. Instructions are strictly paired per thread (an interrupted
// pair re-runs BEFORE or never consumes the stale slot), so a clobbered
// slot is always overwritten before its next use.
const UINT32 MAX_WRITE_ORDINAL = 4; // x64 has at most 2 explicit mem-write ops
TLS_KEY g_write_ea_keys[MAX_WRITE_ORDINAL];

// Instrumentation-time only (instrumentation callbacks are serialized by
// the VM lock): 0 = not tried, 1 = ready, -1 = unavailable.
INT32 g_write_ea_keys_state = 0;

BOOL EnsureWriteEaKeys()
{
    if (g_write_ea_keys_state == 0)
    {
        g_write_ea_keys_state = 1;
        for (UINT32 i = 0; i < MAX_WRITE_ORDINAL; ++i)
        {
            g_write_ea_keys[i] = PIN_CreateThreadDataKey(0);
            if (g_write_ea_keys[i] == INVALID_TLS_KEY)
            {
                g_write_ea_keys_state = -1;
                break;
            }
        }
    }
    return g_write_ea_keys_state == 1;
}

VOID OnMemoryOperandSaveWriteEa(
    THREADID thread_id, ADDRINT memory_address, UINT32 ordinal)
{
    if (ordinal < MAX_WRITE_ORDINAL)
        PIN_SetThreadData(g_write_ea_keys[ordinal],
                          reinterpret_cast<const VOID*>(memory_address), thread_id);
}

VOID OnMemoryOperandValueWrite(
    ADDRINT instruction_address, THREADID thread_id,
    UINT32 ordinal, UINT32 size, UINT32 access, VOID* raw_state)
{
    MemoryOperandValueState* state = static_cast<MemoryOperandValueState*>(raw_state);
    ADDRINT memory_address = 0;
    if (ordinal < MAX_WRITE_ORDINAL)
        memory_address = reinterpret_cast<ADDRINT>(
            PIN_GetThreadData(g_write_ea_keys[ordinal], thread_id));
    state->callback(
        static_cast<uint64_t>(instruction_address),
        static_cast<uint32_t>(thread_id),
        static_cast<uint64_t>(memory_address), size, access,
        SafeReadValue(memory_address, size), state->user_data);
}

} // namespace

PbStatus PbBackendInsInsertCaptureRegs(
    PbInsHandle ins, PbInsCaptureRegsCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    CaptureRegsState* state = PbInternPersistentCallbackState(
        g_capture_regs_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnCaptureRegs),
        IARG_INST_PTR, IARG_THREAD_ID,
#if defined(TARGET_IA32E)
        IARG_REG_VALUE, REG_RCX, IARG_REG_VALUE, REG_RDX,
        IARG_REG_VALUE, REG_R8, IARG_REG_VALUE, REG_R9,
#else
        // ia32 has no r8/r9. Keep the four slots useful by exposing the
        // remaining general-purpose registers as ECX, EDX, EAX, EBX.
        IARG_REG_VALUE, REG_ECX, IARG_REG_VALUE, REG_EDX,
        IARG_REG_VALUE, REG_EAX, IARG_REG_VALUE, REG_EBX,
#endif
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertHookMonitor(
    PbInsHandle ins, PbInsHookMonitorCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    HookMonitorState* state = PbInternPersistentCallbackState(
        g_hook_monitor_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnHookMonitor),
        IARG_INST_PTR, IARG_THREAD_ID,
#if defined(TARGET_IA32E)
        IARG_REG_VALUE, REG_RCX, IARG_REG_VALUE, REG_RDX,
        IARG_REG_VALUE, REG_R8, IARG_REG_VALUE, REG_R9,
        IARG_REG_VALUE, REG_RSP, IARG_REG_VALUE, REG_RAX,
#else
        IARG_REG_VALUE, REG_ECX, IARG_REG_VALUE, REG_EDX,
        IARG_REG_VALUE, REG_EAX, IARG_REG_VALUE, REG_EBX,
        IARG_REG_VALUE, REG_ESP, IARG_REG_VALUE, REG_EAX,
#endif
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertCaptureRegsCtx(
    PbInsHandle ins, PbInsContextCaptureRegsCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ContextCaptureRegsState* state = PbInternPersistentCallbackState(
        g_context_capture_regs_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnContextCaptureRegs),
        IARG_INST_PTR, IARG_THREAD_ID, IARG_CONTEXT,
#if defined(TARGET_IA32E)
        IARG_REG_VALUE, REG_RCX, IARG_REG_VALUE, REG_RDX,
        IARG_REG_VALUE, REG_R8, IARG_REG_VALUE, REG_R9,
#else
        IARG_REG_VALUE, REG_ECX, IARG_REG_VALUE, REG_EDX,
        IARG_REG_VALUE, REG_EAX, IARG_REG_VALUE, REG_EBX,
#endif
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryOperands(
    PbInsHandle ins, PbInsMemoryOperandCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    MemoryOperandState* state = PbInternPersistentCallbackState(
        g_memory_operand_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const INS pins = ToIns(ins);
    const UINT32 count = INS_MemoryOperandCount(pins);
    UINT32 read_count = 0;
    for (UINT32 operand = 0; operand < count; ++operand)
    {
        if (INS_MemoryOperandIsRead(pins, operand))
        {
            const uint32_t access = read_count++ == 0
                ? PB_MEMORY_TYPE_READ : PB_MEMORY_TYPE_READ2;
            INS_InsertPredicatedCall(
                pins, IPOINT_BEFORE, AFUNPTR(OnMemoryOperand),
                IARG_INST_PTR, IARG_THREAD_ID,
                IARG_MEMORYOP_EA, operand, IARG_MEMORYOP_SIZE, operand,
                IARG_UINT32, access, IARG_PTR, state, IARG_END);
        }
        if (INS_MemoryOperandIsWritten(pins, operand))
        {
            INS_InsertPredicatedCall(
                pins, IPOINT_BEFORE, AFUNPTR(OnMemoryOperand),
                IARG_INST_PTR, IARG_THREAD_ID,
                IARG_MEMORYOP_EA, operand, IARG_MEMORYOP_SIZE, operand,
                IARG_UINT32, PB_MEMORY_TYPE_WRITE, IARG_PTR, state, IARG_END);
        }
    }
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryAddressTranslation(
    PbInsHandle ins, PbInsMemoryTranslateCallback callback, void* user_data,
    PbRegId scratch_reg0, PbRegId scratch_reg1)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    REG scratch[2];
    if (!PbPinRegFromId(scratch_reg0, &scratch[0]) ||
        !PbPinRegFromId(scratch_reg1, &scratch[1]) || scratch[0] == scratch[1])
        return PB_ERR_INVALID_ARGUMENT;
    const INS pins = ToIns(ins);
    const UINT32 count = INS_MemoryOperandCount(pins);
    if (count > 2 || INS_HasScatteredMemoryAccess(pins))
        return PB_ERR_UNSUPPORTED;
    if (count == 0)
        return PB_OK;
    MemoryTranslateState* state = PbInternPersistentCallbackState(
        g_memory_translate_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    for (UINT32 operand = 0; operand < count; ++operand)
    {
        const UINT32 operation = INS_MemoryOperandIsWritten(pins, operand)
            ? PB_PIN_MEMOP_STORE : PB_PIN_MEMOP_LOAD;
        INS_InsertPredicatedCall(
            pins, IPOINT_BEFORE, AFUNPTR(OnMemoryTranslate),
            IARG_INST_PTR, IARG_THREAD_ID,
            IARG_MEMORYOP_EA, operand, IARG_MEMORYOP_SIZE, operand,
            IARG_UINT32, operation, IARG_PTR, state,
            IARG_RETURN_REGS, scratch[operand], IARG_END);
        INS_RewriteMemoryOperand(pins, operand, scratch[operand]);
    }
    return PB_OK;
}

PbStatus PbBackendInsInsertExec(
    PbInsHandle ins, PbInsExecCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ExecState* state = PbInternPersistentCallbackState(
        g_exec_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const INS pins = ToIns(ins);
    INS_InsertPredicatedCall(
        pins, IPOINT_BEFORE, AFUNPTR(OnExec),
        IARG_INST_PTR, IARG_THREAD_ID,
        IARG_UINT32, static_cast<UINT32>(INS_Size(pins)),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertBranchEdge(
    PbInsHandle ins, PbInsBranchEdgeCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    BranchEdgeState* state = PbInternPersistentCallbackState(
        g_branch_edge_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    INS_InsertPredicatedCall(
        ToIns(ins), IPOINT_BEFORE, AFUNPTR(OnBranchEdge),
        IARG_INST_PTR, IARG_THREAD_ID,
        IARG_BRANCH_TARGET_ADDR, IARG_BRANCH_TAKEN,
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertCaptureExecBytes(
    PbInsHandle ins, PbInsExecBytesCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    ExecBytesState* state = PbInternPersistentCallbackState(
        g_exec_bytes_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const INS pins = ToIns(ins);
    INS_InsertPredicatedCall(
        pins, IPOINT_BEFORE, AFUNPTR(OnExecBytes),
        IARG_INST_PTR, IARG_THREAD_ID,
        IARG_UINT32, static_cast<UINT32>(INS_Size(pins)),
        IARG_PTR, state, IARG_END);
    return PB_OK;
}

PbStatus PbBackendInsInsertMemoryOperandsValues(
    PbInsHandle ins, PbInsMemoryOperandValueCallback callback, void* user_data)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    MemoryOperandValueState* state = PbInternPersistentCallbackState(
        g_memory_operand_value_states, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const INS pins = ToIns(ins);
    const UINT32 count = INS_MemoryOperandCount(pins);
    const BOOL after_ok = INS_IsValidForIpointAfter(pins);
    UINT32 read_count = 0;
    UINT32 write_ordinal = 0;
    for (UINT32 operand = 0; operand < count; ++operand)
    {
        if (INS_MemoryOperandIsRead(pins, operand))
        {
            const uint32_t access = read_count++ == 0
                ? PB_MEMORY_TYPE_READ : PB_MEMORY_TYPE_READ2;
            INS_InsertPredicatedCall(
                pins, IPOINT_BEFORE, AFUNPTR(OnMemoryOperandValue),
                IARG_INST_PTR, IARG_THREAD_ID,
                IARG_MEMORYOP_EA, operand, IARG_MEMORYOP_SIZE, operand,
                IARG_UINT32, access, IARG_PTR, state, IARG_END);
        }
        if (INS_MemoryOperandIsWritten(pins, operand))
        {
            const UINT32 ordinal = write_ordinal++;
            if (after_ok && ordinal < MAX_WRITE_ORDINAL && EnsureWriteEaKeys())
            {
                // report the just-written value from the fall-through path
                INS_InsertPredicatedCall(
                    pins, IPOINT_BEFORE, AFUNPTR(OnMemoryOperandSaveWriteEa),
                    IARG_THREAD_ID, IARG_MEMORYOP_EA, operand,
                    IARG_UINT32, ordinal, IARG_END);
                INS_InsertPredicatedCall(
                    pins, IPOINT_AFTER, AFUNPTR(OnMemoryOperandValueWrite),
                    IARG_INST_PTR, IARG_THREAD_ID,
                    IARG_UINT32, ordinal,
                    IARG_UINT32, static_cast<UINT32>(INS_MemoryOperandSize(pins, operand)),
                    IARG_UINT32, PB_MEMORY_TYPE_WRITE, IARG_PTR, state, IARG_END);
            }
            else
            {
                // no fall-through (e.g. call): the post-write value cannot be
                // sampled, so report the pre-write content instead
                INS_InsertPredicatedCall(
                    pins, IPOINT_BEFORE, AFUNPTR(OnMemoryOperandValue),
                    IARG_INST_PTR, IARG_THREAD_ID,
                    IARG_MEMORYOP_EA, operand, IARG_MEMORYOP_SIZE, operand,
                    IARG_UINT32, PB_MEMORY_TYPE_WRITE, IARG_PTR, state, IARG_END);
            }
        }
    }
    return PB_OK;
}
