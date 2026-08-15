#include <windows.h>
#include <stdio.h>

typedef LONG NTSTATUS;
typedef NTSTATUS (NTAPI *NtCloseFn)(HANDLE);

int main(void)
{
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    NtCloseFn nt_close = ntdll
        ? (NtCloseFn)GetProcAddress(ntdll, "NtClose")
        : NULL;
    HANDLE event = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!nt_close || !event)
        return 2;

    /* Let the runner resolve the syscall number and load both interceptors. */
    Sleep(4000);
    NTSTATUS first = nt_close(event);
    BOOL survived_first = SetEvent(event);
    NTSTATUS second = nt_close(event);
    BOOL closed_after_second = !CloseHandle(event) && GetLastError() == ERROR_INVALID_HANDLE;
    /* Let both Python observation APIs drain the target calls' edges. */
    Sleep(750);
    printf("syscall_python_demo: first=0x%08lx second=0x%08lx survived=%d closed=%d\n",
           (unsigned long)first, (unsigned long)second,
           (int)survived_first, (int)closed_after_second);
    return first != 0 && (unsigned long)second == 0xC0000022UL &&
           survived_first && closed_after_second ? 0 : 7;
}
