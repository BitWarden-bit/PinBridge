#include "pin.H"
#include "atomic/ops.hpp"

#include "control_internal_exception_backend.h"

#include <cstdlib>

namespace
{

const ADDRINT kTrapFlag = static_cast<ADDRINT>(0x100);
const ADDRINT kInterruptFlag = static_cast<ADDRINT>(0x200);
const ADDRINT kIoplMask = static_cast<ADDRINT>(0x3000);

/* With Pin 3.31 on Windows we observed application-generated TF traps escape
   at a physical code-cache address. Pin documents that JIT mode executes only
   generated code and that POPF terminates a trace; it does not explicitly
   document this TF behavior. Keep compatibility state in Pin THREADID space
   (not OS TLS: the Rust agent is privately mapped by Pin). */
struct SingleStepState
{
    volatile UINT32 enabled;
    volatile UINT32 pending;
};

SingleStepState g_single_step[PIN_MAX_THREADS] = {};
volatile UINT32 g_single_step_global_enabled = 0;
volatile UINT32 g_single_step_registered = 0;

/* POPF terminates a Pin trace, so its architectural successor is normally the
   head of another trace. Remember only those successor addresses: normal
   traces receive no run-time TF check. The table is populated while Pin holds
   its instrumentation lock and read by later instrumentation callbacks. */
const UINT32 kSingleStepBoundaryCapacity = 1u << 16;
volatile ADDRINT g_single_step_boundaries[kSingleStepBoundaryCapacity] = {};

UINT32 AtomicLoad(const volatile UINT32* value)
{
    return ATOMIC::OPS::Load(value, ATOMIC::BARRIER_LD_NEXT);
}

void AtomicStore(volatile UINT32* target, UINT32 value)
{
    ATOMIC::OPS::Store(target, value, ATOMIC::BARRIER_ST_PREV);
}

UINT32 AtomicExchange(volatile UINT32* target, UINT32 value)
{
    return ATOMIC::OPS::Swap(target, value, ATOMIC::BARRIER_SWAP_PREV);
}

bool ValidThread(THREADID thread_id)
{
    return static_cast<UINT32>(thread_id) < PIN_MAX_THREADS;
}

UINT32 PopFlagsWidth(INS ins)
{
    switch (INS_Opcode(ins))
    {
    case XED_ICLASS_POPF:
        return 2;
    case XED_ICLASS_POPFD:
        return 4;
#if defined(TARGET_IA32E)
    case XED_ICLASS_POPFQ:
        return 8;
#endif
    default:
        return 0;
    }
}

UINT32 PushFlagsWidth(INS ins)
{
    switch (INS_Opcode(ins))
    {
    case XED_ICLASS_PUSHF:
        return 2;
    case XED_ICLASS_PUSHFD:
        return 4;
#if defined(TARGET_IA32E)
    case XED_ICLASS_PUSHFQ:
        return 8;
#endif
    default:
        return 0;
    }
}

bool SingleStepEnabled(THREADID thread_id)
{
    return ValidThread(thread_id) &&
        (AtomicLoad(&g_single_step_global_enabled) != 0 ||
         AtomicLoad(&g_single_step[thread_id].enabled) != 0);
}

ADDRINT EmulateUserPopFlags(ADDRINT current, ADDRINT incoming, UINT32 width)
{
    /* CPL3 POPF may change the arithmetic flags, TF, DF, NT and (for the
       32/64-bit forms) AC/ID. IF remains protected unless IOPL==3; IOPL and
       privileged virtualization flags are never copied from the stack. */
    ADDRINT writable = static_cast<ADDRINT>(0x0000000000004dd5); // through NT
    if (width >= 4)
        writable |= static_cast<ADDRINT>(0x0000000000240000); // AC | ID
    if ((current & kIoplMask) == kIoplMask)
        writable |= kInterruptFlag;
    ADDRINT result = (current & ~writable) | (incoming & writable);
    result |= static_cast<ADDRINT>(2); // architectural reserved-one bit
    return result & ~kTrapFlag;
}

ADDRINT NeedsVirtualizePopFlags(
    THREADID thread_id, ADDRINT stack_pointer, UINT32 width)
{
    if (!ValidThread(thread_id) ||
        (width != 2 && width != 4 && width != 8))
        return 0;
    ADDRINT incoming = 0;
    /* A failed probe is deliberately ignored here: the real POPF remains in
       the trace and raises the application's genuine access fault. */
    return PIN_SafeCopy(&incoming,
        reinterpret_cast<const VOID*>(stack_pointer), width) == width &&
        (incoming & kTrapFlag) != 0 && SingleStepEnabled(thread_id);
}

VOID VirtualizePopFlags(
    CONTEXT* context, THREADID thread_id, ADDRINT next_ip, UINT32 width)
{
    if (!context || !ValidThread(thread_id))
        return;
    const ADDRINT stack_pointer = PIN_GetContextReg(context, REG_STACK_PTR);
    ADDRINT incoming = 0;
    if (PIN_SafeCopy(&incoming,
            reinterpret_cast<const VOID*>(stack_pointer), width) != width)
        return;
    const ADDRINT current = PIN_GetContextReg(context, REG_GFLAGS);
    PIN_SetContextReg(context, REG_GFLAGS,
        EmulateUserPopFlags(current, incoming, width));
    PIN_SetContextReg(context, REG_STACK_PTR, stack_pointer + width);
    PIN_SetContextReg(context, REG_INST_PTR, next_ip);
    SingleStepState* state = &g_single_step[thread_id];
    AtomicStore(&state->pending, 1);
    /* Skip the physical POPF. It would set physical TF in Pin's code cache
       before an application context exists. */
    PIN_ExecuteAt(context); // never returns
}

ADDRINT PIN_FAST_ANALYSIS_CALL HasPendingSingleStep(THREADID thread_id)
{
    return ValidThread(thread_id) &&
        AtomicLoad(&g_single_step[thread_id].pending) != 0;
}

VOID RestorePushedTrapFlag(const CONTEXT* context, UINT32 width)
{
    if (!context || (width != 2 && width != 4 && width != 8))
        return;
    const ADDRINT stack_pointer = PIN_GetContextReg(context, REG_STACK_PTR);
    ADDRINT pushed_flags = 0;
    if (PIN_SafeCopy(&pushed_flags,
            reinterpret_cast<const VOID*>(stack_pointer), width) != width)
        return;
    pushed_flags |= kTrapFlag;
    /* The real PUSHF just wrote the same stack slot successfully. This patch
       restores the logical TF bit hidden from physical code-cache RFLAGS. */
    PIN_SafeCopy(reinterpret_cast<VOID*>(stack_pointer), &pushed_flags, width);
}

VOID RaisePendingSingleStep(
    CONTEXT* context, THREADID thread_id, UINT32 pushed_flags_width)
{
    if (!ValidThread(thread_id) ||
        AtomicExchange(&g_single_step[thread_id].pending, 0) == 0)
        return;

    /* Clear first: PIN_RaiseException never returns and the application may
       resume through an arbitrary VEH/SEH path. This is also the recursion
       guard if Pin reports the synthesized event through another callback. */
    CONTEXT application_context;
    PIN_SaveContext(context, &application_context);
    RestorePushedTrapFlag(&application_context, pushed_flags_width);
    const ADDRINT application_ip =
        PIN_GetContextReg(&application_context, REG_INST_PTR);
    PIN_SetContextReg(&application_context, REG_GFLAGS,
        PIN_GetContextReg(&application_context, REG_GFLAGS) | kTrapFlag);

    EXCEPTION_INFO exception_info;
    PIN_InitExceptionInfo(
        &exception_info, EXCEPTCODE_DBG_SINGLE_STEP_TRAP, application_ip);
    PIN_RaiseException(&application_context, thread_id, &exception_info);
}

VOID InstrumentSingleStepBoundary(INS ins)
{
    if (!INS_Valid(ins))
        return;
    const UINT32 pushed_flags_width = PushFlagsWidth(ins);
    if (INS_IsValidForIpointAfter(ins))
    {
        INS_InsertIfCall(ins, IPOINT_AFTER, AFUNPTR(HasPendingSingleStep),
            IARG_FAST_ANALYSIS_CALL, IARG_THREAD_ID, IARG_END);
        INS_InsertThenCall(ins, IPOINT_AFTER, AFUNPTR(RaisePendingSingleStep),
            IARG_CONTEXT, IARG_THREAD_ID, IARG_UINT32, pushed_flags_width,
            IARG_END);
    }
    if (INS_IsValidForIpointTakenBranch(ins))
    {
        INS_InsertIfCall(ins, IPOINT_TAKEN_BRANCH, AFUNPTR(HasPendingSingleStep),
            IARG_FAST_ANALYSIS_CALL, IARG_THREAD_ID, IARG_END);
        INS_InsertThenCall(ins, IPOINT_TAKEN_BRANCH, AFUNPTR(RaisePendingSingleStep),
            IARG_CONTEXT, IARG_THREAD_ID, IARG_UINT32, 0, IARG_END);
    }
}

UINT32 BoundaryHash(ADDRINT address)
{
    address ^= address >> 17;
    address *= static_cast<ADDRINT>(0xed5ad4bbU);
    address ^= address >> 11;
    return static_cast<UINT32>(address) & (kSingleStepBoundaryCapacity - 1);
}

bool RegisterSingleStepBoundary(ADDRINT address)
{
    if (address == 0)
        return false;
    UINT32 slot = BoundaryHash(address);
    for (UINT32 probe = 0; probe < kSingleStepBoundaryCapacity; ++probe)
    {
        volatile ADDRINT* cell = &g_single_step_boundaries[slot];
        const ADDRINT current = ATOMIC::OPS::Load<ADDRINT>(cell,
            ATOMIC::BARRIER_LD_NEXT);
        if (current == address)
            return true;
        if (current == 0 && ATOMIC::OPS::CompareAndDidSwap<ADDRINT>(
                cell, 0, address, ATOMIC::BARRIER_CS_PREV))
            return true;
        slot = (slot + 1) & (kSingleStepBoundaryCapacity - 1);
    }
    return false;
}

bool IsSingleStepBoundary(ADDRINT address)
{
    if (address == 0)
        return false;
    UINT32 slot = BoundaryHash(address);
    for (UINT32 probe = 0; probe < kSingleStepBoundaryCapacity; ++probe)
    {
        const ADDRINT current = ATOMIC::OPS::Load<ADDRINT>(
            &g_single_step_boundaries[slot], ATOMIC::BARRIER_LD_NEXT);
        if (current == address)
            return true;
        if (current == 0)
            return false;
        slot = (slot + 1) & (kSingleStepBoundaryCapacity - 1);
    }
    return false;
}

VOID InstrumentTrapFlagWriter(INS ins, VOID*)
{
    if (IsSingleStepBoundary(INS_Address(ins)))
        InstrumentSingleStepBoundary(ins);

    const UINT32 width = PopFlagsWidth(ins);
    if (width == 0)
        return;

    const ADDRINT next_ip = INS_NextAddress(ins);
    if (!RegisterSingleStepBoundary(next_ip))
        return;

    /* The successor may already have a cached trace (for example after an
       earlier branch). This call runs at JIT instrumentation time, never in an
       application analysis callback, and makes that trace pick up the exact
       boundary check on its next execution. */
    PIN_RemoveInstrumentationInRange(next_ip, next_ip);

    /* Ordinary POPF stays native. Only a readable TF=1 image enters the
       compatibility path; all other POPF instructions, including access
       faults, execute exactly as Pin translated them. */
    INS_InsertIfCall(ins, IPOINT_BEFORE, AFUNPTR(NeedsVirtualizePopFlags),
        IARG_THREAD_ID, IARG_REG_VALUE, REG_STACK_PTR, IARG_UINT32, width,
        IARG_END);
    INS_InsertThenCall(ins, IPOINT_BEFORE, AFUNPTR(VirtualizePopFlags),
        IARG_CONTEXT, IARG_THREAD_ID, IARG_ADDRINT, next_ip,
        IARG_UINT32, width, IARG_END);
}

struct CallbackState
{
    PbInternalExceptionCallback callback;
    void* user_data;
    PbThreadId thread_id;
};

EXCEPT_HANDLING_RESULT OnInternalException(
    THREADID thread_id, EXCEPTION_INFO* exception_info,
    PHYSICAL_CONTEXT* physical_context, VOID* raw_state)
{
    CallbackState* state = static_cast<CallbackState*>(raw_state);
    const PbExceptHandlingResult result = state->callback(
        static_cast<PbThreadId>(thread_id),
        reinterpret_cast<PbExceptionInfoHandle>(exception_info),
        reinterpret_cast<PbPhysicalContextHandle>(physical_context),
        state->user_data);
    if (result != PB_EHR_HANDLED && result != PB_EHR_UNHANDLED &&
        result != PB_EHR_CONTINUE_SEARCH)
        return EHR_UNHANDLED;
    return static_cast<EXCEPT_HANDLING_RESULT>(result);
}

CallbackState* AllocateState(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data)
{
    CallbackState* state = static_cast<CallbackState*>(std::malloc(sizeof(CallbackState)));
    if (state)
    {
        state->callback = callback;
        state->user_data = user_data;
        state->thread_id = thread_id;
    }
    return state;
}

} // namespace

PbStatus PbBackendAddInternalExceptionHandler(
    PbInternalExceptionCallback callback, void* user_data, uint64_t* out_callback)
{
    CallbackState* state = AllocateState(0, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    const PIN_CALLBACK pin_callback = PIN_AddInternalExceptionHandler(OnInternalException, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

PbStatus PbBackendEnableSingleStepPassthrough(uint64_t* out_callback)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const PIN_CALLBACK ins_callback =
        INS_AddInstrumentFunction(InstrumentTrapFlagWriter, 0);
    if (ins_callback == PIN_CALLBACK_INVALID)
        return PB_ERR_INTERNAL;
    for (UINT32 slot = 0; slot < kSingleStepBoundaryCapacity; ++slot)
        ATOMIC::OPS::Store<ADDRINT>(&g_single_step_boundaries[slot], 0,
            ATOMIC::BARRIER_ST_PREV);
    for (UINT32 thread_id = 0; thread_id < PIN_MAX_THREADS; ++thread_id)
    {
        AtomicStore(&g_single_step[thread_id].enabled, 0);
        AtomicStore(&g_single_step[thread_id].pending, 0);
    }
    /* Preserve the original public API behavior. Scoped users register the
       bridge and immediately turn this process-wide gate off before Pin starts
       the application. */
    AtomicStore(&g_single_step_global_enabled, 1);
    AtomicStore(&g_single_step_registered, 1);
    *out_callback =
        static_cast<uint64_t>(reinterpret_cast<uintptr_t>(ins_callback));
    return PB_OK;
}

PbStatus PbBackendSetSingleStepPassthrough(
    PbThreadId thread_id, uint8_t enabled)
{
    if (AtomicLoad(&g_single_step_registered) == 0)
        return PB_ERR_INVALID_STATE;
    if (thread_id == PB_INVALID_THREAD_ID)
    {
        AtomicStore(&g_single_step_global_enabled, enabled);
        return PB_OK;
    }
    if (!ValidThread(static_cast<THREADID>(thread_id)))
        return PB_ERR_INVALID_ARGUMENT;
    AtomicStore(&g_single_step[thread_id].enabled, enabled);
    return PB_OK;
}

PbStatus PbBackendTryStart(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    uint64_t* out_scope)
{
    CallbackState* state = AllocateState(thread_id, callback, user_data);
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    PIN_TryStart(static_cast<THREADID>(thread_id), OnInternalException, state);
    *out_scope = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(state));
    return PB_OK;
}

PbStatus PbBackendTryEnd(PbThreadId thread_id, uint64_t scope)
{
    CallbackState* state = reinterpret_cast<CallbackState*>(static_cast<uintptr_t>(scope));
    if (state->thread_id != thread_id)
        return PB_ERR_INVALID_STATE;
    PIN_TryEnd(static_cast<THREADID>(thread_id));
    std::free(state);
    return PB_OK;
}
