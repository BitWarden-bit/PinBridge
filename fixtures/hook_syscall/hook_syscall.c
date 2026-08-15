/* Small in-repo target for NTDLL export hooks and syscall observation. */
#include <windows.h>
#include <stdio.h>

typedef LONG NTSTATUS;
typedef NTSTATUS (NTAPI *NtYieldExecutionFn)(void);
typedef NTSTATUS (NTAPI *NtCloseFn)(HANDLE);
typedef NTSTATUS (NTAPI *NtQueryInformationProcessFn)(
    HANDLE, ULONG, PVOID, ULONG, PULONG);

typedef struct _PROCESS_BASIC_INFORMATION_MIN {
    PVOID reserved1;
    PVOID peb_base_address;
    PVOID reserved2[2];
    ULONG_PTR unique_process_id;
    PVOID reserved3;
} PROCESS_BASIC_INFORMATION_MIN;

int main(void)
{
    HMODULE ntdll = GetModuleHandleA("ntdll.dll");
    NtYieldExecutionFn nt_yield;
    NtCloseFn nt_close;
    NtQueryInformationProcessFn nt_query;
    PROCESS_BASIC_INFORMATION_MIN info = {0};
    ULONG returned = 0;
    HANDLE event_handle;
    NTSTATUS close_status = 0;
    NTSTATUS yield_status;
    NTSTATUS query_status;

    if (!ntdll) {
        puts("hook_syscall: ntdll missing");
        return 2;
    }
    nt_yield = (NtYieldExecutionFn)GetProcAddress(ntdll, "NtYieldExecution");
    nt_close = (NtCloseFn)GetProcAddress(ntdll, "NtClose");
    nt_query = (NtQueryInformationProcessFn)GetProcAddress(
        ntdll, "NtQueryInformationProcess");
    if (!nt_yield || !nt_close || !nt_query) {
        puts("hook_syscall: required exports missing");
        return 3;
    }

    /* Give the controller time to enumerate exports and arm hooks. */
    Sleep(30000);
    yield_status = nt_yield();
    query_status = nt_query(GetCurrentProcess(), 0, &info,
                             (ULONG)sizeof(info), &returned);
    event_handle = CreateEventA(NULL, TRUE, FALSE, NULL);
    if (event_handle) {
        close_status = nt_close(event_handle);
    }
    Sleep(10000);

    printf("hook_syscall: pid=%lu yield=0x%08lx query=0x%08lx close=0x%08lx peb=%p\n",
           (unsigned long)GetCurrentProcessId(),
           (unsigned long)yield_status,
           (unsigned long)query_status,
           (unsigned long)close_status,
           info.peb_base_address);
    return 0;
}
