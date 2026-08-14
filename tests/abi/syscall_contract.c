#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_entry_calls;
static uint32_t g_exit_calls;

static void PB_CALL OnEntry(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard, void* user_data)
{
    if (thread_id == 7 && context != 0 &&
        standard == PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST &&
        user_data == &g_entry_calls)
        ++g_entry_calls;
}

static void PB_CALL OnExit(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard, void* user_data)
{
    if (thread_id == 7 && context != 0 &&
        standard == PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST &&
        user_data == &g_exit_calls)
        ++g_exit_calls;
}

int main(void)
{
    PbContextHandle context = (PbContextHandle)(uintptr_t)0x5000;
    PbConstContextHandle const_context =
        (PbConstContextHandle)(uintptr_t)0x5000;
    PbCallbackHandle entry = {0};
    PbCallbackHandle exit_callback = {0};
    uint64_t value = 0;
    uint32_t index;

    if (sizeof(PbSyscallStandard) != 4 ||
        PB_SYSCALL_STANDARD_INVALID != 0 ||
        PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST != 8 ||
        PB_SYSCALL_STANDARD_WINDOWS_INT != 11)
        return 1;
    if (pb_pin_add_syscall_entry_function(
            OnEntry, &g_entry_calls, &entry) != PB_OK ||
        entry.opaque == PB_CALLBACK_INVALID_OPAQUE || g_entry_calls != 1)
        return 2;
    if (pb_pin_add_syscall_exit_function(
            OnExit, &g_exit_calls, &exit_callback) != PB_OK ||
        exit_callback.opaque == PB_CALLBACK_INVALID_OPAQUE || g_exit_calls != 1)
        return 3;
    if (pb_pin_get_syscall_argument(
            const_context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST, 0,
            &value) != PB_OK || value != UINT64_C(0xA0))
        return 4;
    if (pb_pin_get_syscall_number(
            const_context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            &value) != PB_OK || value != UINT64_C(0xA1))
        return 5;
    if (pb_pin_get_syscall_return(
            const_context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            &value) != PB_OK || value != UINT64_C(0xA2))
        return 6;
    if (pb_pin_get_syscall_errno(
            const_context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            &value) != PB_OK || value != UINT64_C(0xA3))
        return 7;
    if (pb_pin_set_syscall_argument(
            context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST, 0,
            UINT64_C(0xB0)) != PB_OK ||
        pb_pin_set_syscall_number(
            context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            UINT64_C(0xB1)) != PB_OK ||
        pb_pin_set_syscall_return(
            context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            UINT64_C(0xB2)) != PB_OK ||
        pb_pin_set_syscall_errno(
            context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST,
            UINT64_C(0xB3)) != PB_OK)
        return 8;
    if (pb_pin_replay_syscall_entry(
            7, context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST) != PB_OK ||
        pb_pin_replay_syscall_exit(
            7, context, PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST) != PB_OK)
        return 9;
    for (index = 0; index != 12; ++index) {
        if (index == 0 && PB_SYSCALL_STANDARD_INVALID != index)
            return 10;
    }
    if (pb_pin_add_syscall_entry_function(0, 0, &entry) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_add_syscall_exit_function(OnExit, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_syscall_argument(0, PB_SYSCALL_STANDARD_INVALID, 0, &value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_syscall_number(const_context, PB_SYSCALL_STANDARD_INVALID, &value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_syscall_return(0, PB_SYSCALL_STANDARD_INVALID, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 11;
    return 0;
}
