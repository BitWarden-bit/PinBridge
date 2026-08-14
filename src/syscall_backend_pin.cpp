#include "pin.H"

#include "syscall_backend.h"

#include <cstdlib>

namespace
{

static_assert(sizeof(SYSCALL_STANDARD) == sizeof(uint32_t),
              "Pin syscall standard width changed");
static_assert(SYSCALL_STANDARD_INVALID == 0 &&
              SYSCALL_STANDARD_WINDOWS_INT == 11,
              "Pin syscall standard values changed");

template< typename Callback > struct CallbackState
{
    Callback callback;
    void* user_data;
};

template< typename Callback > PIN_CALLBACK AddCallback(
    Callback callback, void* user_data,
    PIN_CALLBACK (*registration)(VOID (*)(THREADID, CONTEXT*, SYSCALL_STANDARD, VOID*), VOID*))
{
    CallbackState<Callback>* state =
        static_cast<CallbackState<Callback>*>(std::malloc(sizeof(CallbackState<Callback>)));
    if (!state)
        return PIN_CALLBACK_INVALID;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK result = registration(
        [](THREADID thread_id, CONTEXT* context, SYSCALL_STANDARD standard, VOID* raw) {
            CallbackState<Callback>* callback_state =
                static_cast<CallbackState<Callback>*>(raw);
            callback_state->callback(static_cast<PbThreadId>(thread_id),
                reinterpret_cast<PbContextHandle>(context),
                static_cast<PbSyscallStandard>(standard), callback_state->user_data);
        }, state);
    if (result == PIN_CALLBACK_INVALID)
        std::free(state);
    return result;
}

bool ProbeRejected(void)
{
    return PIN_IsProbeMode() != 0;
}

} // namespace

PbStatus PbBackendAddSyscallEntryFunction(
    PbSyscallEntryCallback callback, void* user_data, uint64_t* out_callback)
{
    if (ProbeRejected())
        return PB_ERR_INVALID_STATE;
    const PIN_CALLBACK result = AddCallback(
        callback, user_data, PIN_AddSyscallEntryFunction);
    if (result == PIN_CALLBACK_INVALID)
        return PB_ERR_INTERNAL;
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(result));
    return PB_OK;
}

PbStatus PbBackendAddSyscallExitFunction(
    PbSyscallExitCallback callback, void* user_data, uint64_t* out_callback)
{
    if (ProbeRejected())
        return PB_ERR_INVALID_STATE;
    const PIN_CALLBACK result = AddCallback(
        callback, user_data, PIN_AddSyscallExitFunction);
    if (result == PIN_CALLBACK_INVALID)
        return PB_ERR_INTERNAL;
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(result));
    return PB_OK;
}

#define PB_NATIVE_GET(name) \
PbStatus PbBackendGetSyscall##name( \
    PbConstContextHandle context, PbSyscallStandard standard, uint64_t* out_value) \
{ \
    if (ProbeRejected()) return PB_ERR_INVALID_STATE; \
    *out_value = static_cast<uint64_t>(PIN_GetSyscall##name( \
        reinterpret_cast<const CONTEXT*>(context), \
        static_cast<SYSCALL_STANDARD>(standard))); \
    return PB_OK; \
}

PB_NATIVE_GET(Errno)
PB_NATIVE_GET(Number)
PB_NATIVE_GET(Return)

PbStatus PbBackendGetSyscallArgument(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t* out_value)
{
    if (ProbeRejected())
        return PB_ERR_INVALID_STATE;
    *out_value = static_cast<uint64_t>(PIN_GetSyscallArgument(
        reinterpret_cast<const CONTEXT*>(context),
        static_cast<SYSCALL_STANDARD>(standard), static_cast<UINT32>(arg_num)));
    return PB_OK;
}

#define PB_NATIVE_REPLAY(name) \
PbStatus PbBackendReplaySyscall##name( \
    PbThreadId thread_id, PbContextHandle context, PbSyscallStandard standard) \
{ \
    if (ProbeRejected()) return PB_ERR_INVALID_STATE; \
    PIN_ReplaySyscall##name(static_cast<THREADID>(thread_id), \
        reinterpret_cast<CONTEXT*>(context), static_cast<SYSCALL_STANDARD>(standard)); \
    return PB_OK; \
}

PB_NATIVE_REPLAY(Entry)
PB_NATIVE_REPLAY(Exit)

#define PB_NATIVE_SET(name) \
PbStatus PbBackendSetSyscall##name( \
    PbContextHandle context, PbSyscallStandard standard, uint64_t value) \
{ \
    if (ProbeRejected()) return PB_ERR_INVALID_STATE; \
    PIN_SetSyscall##name(reinterpret_cast<CONTEXT*>(context), \
        static_cast<SYSCALL_STANDARD>(standard), static_cast<ADDRINT>(value)); \
    return PB_OK; \
}

PB_NATIVE_SET(Errno)
PB_NATIVE_SET(Number)
PB_NATIVE_SET(Return)

PbStatus PbBackendSetSyscallArgument(
    PbContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t value)
{
    if (ProbeRejected())
        return PB_ERR_INVALID_STATE;
    PIN_SetSyscallArgument(reinterpret_cast<CONTEXT*>(context),
        static_cast<SYSCALL_STANDARD>(standard), static_cast<UINT32>(arg_num),
        static_cast<ADDRINT>(value));
    return PB_OK;
}
