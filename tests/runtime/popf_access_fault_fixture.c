#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <malloc.h>

static volatile LONG g_access_faults;
static ULONG_PTR g_popf_ip;
static ULONG_PTR g_resume_ip;
static ULONG_PTR g_fault_address;

static LONG CALLBACK OnException(EXCEPTION_POINTERS* pointers)
{
    if (pointers->ExceptionRecord->ExceptionCode != EXCEPTION_ACCESS_VIOLATION)
        return EXCEPTION_CONTINUE_SEARCH;
#if defined(_M_X64)
    if (pointers->ContextRecord->Rip != g_popf_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    g_fault_address = pointers->ExceptionRecord->ExceptionInformation[1];
    pointers->ContextRecord->Rsp = pointers->ContextRecord->R11;
    pointers->ContextRecord->Rip = g_resume_ip;
#else
    if (pointers->ContextRecord->Eip != (DWORD)g_popf_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    g_fault_address = pointers->ExceptionRecord->ExceptionInformation[1];
    pointers->ContextRecord->Esp = (DWORD)g_safe_stack;
    pointers->ContextRecord->Eip = (DWORD)g_resume_ip;
#endif
    InterlockedIncrement(&g_access_faults);
    return EXCEPTION_CONTINUE_EXECUTION;
}

int main(int argc, char** argv)
{
#if defined(_M_X64)
    /* mov r11,rsp; mov rsp,rcx; popfq; mov rsp,r11; ret */
    static const unsigned char code[] = {
        0x49, 0x89, 0xe3, 0x48, 0x89, 0xcc, 0x9d,
        0x4c, 0x89, 0xdc, 0xc3
    };
    const SIZE_T popf_offset = 6;
    const SIZE_T resume_offset = 7;
#else
    /* x86 version passes the invalid address on the stack and is not needed
       by the current x64 Pin regression job. */
    return 77;
#endif
    unsigned char* memory = (unsigned char*)VirtualAlloc(0, sizeof(code),
        MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    void* handler = AddVectoredExceptionHandler(1, OnException);
    if (!memory || !handler)
        return 2;
    memcpy(memory, code, sizeof(code));
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));
    g_popf_ip = (ULONG_PTR)memory + popf_offset;
    g_resume_ip = (ULONG_PTR)memory + resume_offset;

#if defined(_M_X64)
    /* Keep the bad RSP inside this thread's real stack range. The lower half
       of the committed scratch area remains writable for Windows exception
       dispatch while POPF reads from the protected page above it. */
    volatile unsigned char* scratch =
        (volatile unsigned char*)_alloca(256 * 1024);
    SIZE_T index;
    for (index = 0; index < 256 * 1024; index += 0x1000)
        scratch[index] = 0;
    unsigned char* inaccessible = (unsigned char*)
        (((ULONG_PTR)scratch + 128 * 1024 + 0xfff) & ~(ULONG_PTR)0xfff);
    DWORD old_protect = 0;
    if (!VirtualProtect(inaccessible, 0x1000, PAGE_NOACCESS, &old_protect))
        return 4;
    /* The generated routine saves its valid caller stack in R11 before
       pointing RSP at the inaccessible page. The handler restores both RSP
       and RIP, so exception dispatch does not need the invalid application
       stack to return. */
    __try {
        ((void (WINAPI*)(void*))memory)(inaccessible);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        /* A VEH should own the expected fault. This guard turns an unexpected
           dispatch failure into a deterministic test failure. */
        return 3;
    }
    {
        DWORD ignored = 0;
        VirtualProtect(inaccessible, 0x1000, old_protect, &ignored);
    }
#endif

    RemoveVectoredExceptionHandler(handler);
    VirtualFree(memory, 0, MEM_RELEASE);
    printf("access_fault_count=%ld fault_address=%p\n", g_access_faults,
        (void*)g_fault_address);
    if (argc == 2)
    {
        const unsigned long wait_ms = strtoul(argv[1], 0, 10);
        if (wait_ms > 0 && wait_ms <= 60000)
            Sleep((DWORD)wait_ms);
    }
    return g_access_faults == 1 && g_fault_address == (ULONG_PTR)inaccessible
        ? 0 : 1;
}
