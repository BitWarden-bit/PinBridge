#include "pin.H"

#include "control_internal_exception_backend.h"

#include <cstdlib>

namespace
{

const ADDRINT kTrapFlag = static_cast<ADDRINT>(0x100);
const ADDRINT kInterruptFlag = static_cast<ADDRINT>(0x200);
const ADDRINT kIoplMask = static_cast<ADDRINT>(0x3000);

/* Pin 3.31 documents application-generated TF traps as unsupported and lets
   the host #DB escape in the translated code cache. Keep the compatibility
   state in Pin THREADID space (not OS TLS: the Rust agent is privately mapped
   by Pin and deliberately avoids loader TLS for the same reason). */
struct SingleStepState
{
    UINT32 pending;
};

SingleStepState g_single_step[PIN_MAX_THREADS] = {};

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

ADDRINT ReadApplicationFlags(ADDRINT stack_pointer, UINT32 width, ADDRINT* value)
{
    *value = 0;
    if (width != 2 && width != 4 && width != 8)
        return 0;
    return PIN_SafeCopy(value, reinterpret_cast<const VOID*>(stack_pointer), width) == width;
}

ADDRINT NeedsTrapFlagVirtualization(ADDRINT stack_pointer, UINT32 width)
{
    ADDRINT flags = 0;
    return ReadApplicationFlags(stack_pointer, width, &flags) &&
        (flags & kTrapFlag) != 0;
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

VOID VirtualizePopFlags(
    CONTEXT* context, THREADID thread_id, ADDRINT next_ip, UINT32 width)
{
    if (!ValidThread(thread_id))
        return;
    const ADDRINT stack_pointer = PIN_GetContextReg(context, REG_STACK_PTR);
    ADDRINT incoming = 0;
    if (!ReadApplicationFlags(stack_pointer, width, &incoming) ||
        (incoming & kTrapFlag) == 0)
        return;

    const ADDRINT current = PIN_GetContextReg(context, REG_GFLAGS);
    PIN_SetContextReg(context, REG_GFLAGS,
        EmulateUserPopFlags(current, incoming, width));
    PIN_SetContextReg(context, REG_STACK_PTR, stack_pointer + width);
    PIN_SetContextReg(context, REG_INST_PTR, next_ip);
    g_single_step[thread_id].pending = 1;

    /* Skip the real POPF. Executing it in the code cache would set physical
       TF and trap inside Pin before an application CONTEXT exists. */
    PIN_ExecuteAt(context); // never returns
}

ADDRINT HasPendingSingleStep(THREADID thread_id)
{
    return ValidThread(thread_id) && g_single_step[thread_id].pending != 0;
}

VOID RaisePendingSingleStep(CONTEXT* context, THREADID thread_id)
{
    if (!ValidThread(thread_id) || g_single_step[thread_id].pending == 0)
        return;

    /* Clear first: PIN_RaiseException never returns and the application may
       resume through an arbitrary VEH/SEH path. This is also the recursion
       guard if Pin reports the synthesized event through another callback. */
    g_single_step[thread_id].pending = 0;

    CONTEXT application_context;
    PIN_SaveContext(context, &application_context);
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
    if (INS_IsValidForIpointAfter(ins))
    {
        INS_InsertIfCall(ins, IPOINT_AFTER, AFUNPTR(HasPendingSingleStep),
            IARG_THREAD_ID, IARG_END);
        INS_InsertThenCall(ins, IPOINT_AFTER, AFUNPTR(RaisePendingSingleStep),
            IARG_CONTEXT, IARG_THREAD_ID, IARG_END);
    }
    if (INS_IsValidForIpointTakenBranch(ins))
    {
        INS_InsertIfCall(ins, IPOINT_TAKEN_BRANCH, AFUNPTR(HasPendingSingleStep),
            IARG_THREAD_ID, IARG_END);
        INS_InsertThenCall(ins, IPOINT_TAKEN_BRANCH, AFUNPTR(RaisePendingSingleStep),
            IARG_CONTEXT, IARG_THREAD_ID, IARG_END);
    }
}

VOID InstrumentTrapFlagWriter(INS ins, VOID*)
{
    const UINT32 width = PopFlagsWidth(ins);
    if (width == 0)
        return;

    const ADDRINT next_ip = INS_NextAddress(ins);
    INS_InsertIfCall(ins, IPOINT_BEFORE, AFUNPTR(NeedsTrapFlagVirtualization),
        IARG_REG_VALUE, REG_STACK_PTR, IARG_UINT32, width, IARG_END);
    INS_InsertThenCall(ins, IPOINT_BEFORE, AFUNPTR(VirtualizePopFlags),
        IARG_CONTEXT, IARG_THREAD_ID, IARG_ADDRINT, next_ip,
        IARG_UINT32, width, IARG_END);

    /* Normal case: POPF and its successor share a trace. Deliver #DB after
       that successor, when IARG_CONTEXT already points at the architectural
       next instruction (including a taken branch target). */
    InstrumentSingleStepBoundary(INS_Next(ins));
}

VOID InstrumentTraceFallback(TRACE trace, VOID*)
{
    /* POPF can terminate a trace. In that case its successor has no INS_Next
       handle above, so the first instruction of the following trace becomes
       the one instruction executed before the synthesized #DB. */
    const BBL head = TRACE_BblHead(trace);
    if (BBL_Valid(head))
        InstrumentSingleStepBoundary(BBL_InsHead(head));
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
    const PIN_CALLBACK trace_callback =
        TRACE_AddInstrumentFunction(InstrumentTraceFallback, 0);
    const PIN_CALLBACK ins_callback =
        INS_AddInstrumentFunction(InstrumentTrapFlagWriter, 0);
    if (trace_callback == PIN_CALLBACK_INVALID ||
        ins_callback == PIN_CALLBACK_INVALID)
        return PB_ERR_INTERNAL;
    *out_callback =
        static_cast<uint64_t>(reinterpret_cast<uintptr_t>(ins_callback));
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
