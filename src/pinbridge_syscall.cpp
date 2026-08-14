#include "pinbridge/pinbridge.h"

#include "syscall_backend.h"

namespace
{

template< typename Function > PbStatus GuardSyscall(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool ValidStandard(PbSyscallStandard standard)
{
    return standard > PB_SYSCALL_STANDARD_INVALID &&
        standard <= PB_SYSCALL_STANDARD_WINDOWS_INT;
}

template< typename Callback, typename Backend > PbStatus AddCallback(
    Callback callback, void* user_data, PbCallbackHandle* out_callback,
    Backend backend)
{
    if (out_callback)
        out_callback->opaque = PB_CALLBACK_INVALID_OPAQUE;
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardSyscall([&]() {
        return backend(callback, user_data, &out_callback->opaque);
    });
}

} // namespace

PbStatus PB_CALL pb_pin_add_syscall_entry_function(
    PbSyscallEntryCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        PbBackendAddSyscallEntryFunction);
}

PbStatus PB_CALL pb_pin_add_syscall_exit_function(
    PbSyscallExitCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    return AddCallback(callback, user_data, out_callback,
        PbBackendAddSyscallExitFunction);
}

#define PB_SYSCALL_GET(public_name, backend_name) \
PbStatus PB_CALL pb_pin_get_syscall_##public_name( \
    PbConstContextHandle context, PbSyscallStandard standard, uint64_t* out_value) \
{ \
    if (!context || !ValidStandard(standard) || !out_value) \
        return PB_ERR_INVALID_ARGUMENT; \
    *out_value = 0; \
    return GuardSyscall([&]() { \
        return PbBackendGetSyscall##backend_name(context, standard, out_value); \
    }); \
}

PB_SYSCALL_GET(errno, Errno)
PB_SYSCALL_GET(number, Number)
PB_SYSCALL_GET(return, Return)

PbStatus PB_CALL pb_pin_get_syscall_argument(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t* out_value)
{
    if (!context || !ValidStandard(standard) || !out_value)
        return PB_ERR_INVALID_ARGUMENT;
    *out_value = 0;
    return GuardSyscall([&]() {
        return PbBackendGetSyscallArgument(
            context, standard, arg_num, out_value);
    });
}

#define PB_SYSCALL_REPLAY(public_name, backend_name) \
PbStatus PB_CALL pb_pin_replay_syscall_##public_name( \
    PbThreadId thread_id, PbContextHandle context, PbSyscallStandard standard) \
{ \
    if (!context || !ValidStandard(standard)) \
        return PB_ERR_INVALID_ARGUMENT; \
    return GuardSyscall([&]() { \
        return PbBackendReplaySyscall##backend_name(thread_id, context, standard); \
    }); \
}

PB_SYSCALL_REPLAY(entry, Entry)
PB_SYSCALL_REPLAY(exit, Exit)

#define PB_SYSCALL_SET(public_name, backend_name) \
PbStatus PB_CALL pb_pin_set_syscall_##public_name( \
    PbContextHandle context, PbSyscallStandard standard, uint64_t value) \
{ \
    if (!context || !ValidStandard(standard)) \
        return PB_ERR_INVALID_ARGUMENT; \
    return GuardSyscall([&]() { \
        return PbBackendSetSyscall##backend_name(context, standard, value); \
    }); \
}

PB_SYSCALL_SET(errno, Errno)
PB_SYSCALL_SET(number, Number)
PB_SYSCALL_SET(return, Return)

PbStatus PB_CALL pb_pin_set_syscall_argument(
    PbContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t value)
{
    if (!context || !ValidStandard(standard))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardSyscall([&]() {
        return PbBackendSetSyscallArgument(context, standard, arg_num, value);
    });
}
