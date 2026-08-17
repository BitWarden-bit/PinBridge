#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static volatile LONG g_single_steps;
static ULONG_PTR g_expected_ip;

static LONG CALLBACK OnException(EXCEPTION_POINTERS* pointers)
{
    if (pointers->ExceptionRecord->ExceptionCode != EXCEPTION_SINGLE_STEP)
        return EXCEPTION_CONTINUE_SEARCH;
#if defined(_M_X64)
    if (pointers->ContextRecord->Rip != g_expected_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    pointers->ContextRecord->EFlags &= ~0x100u;
#else
    if (pointers->ContextRecord->Eip != (DWORD)g_expected_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    pointers->ContextRecord->EFlags &= ~0x100u;
#endif
    InterlockedIncrement(&g_single_steps);
    return EXCEPTION_CONTINUE_EXECUTION;
}

int main(int argc, char** argv)
{
#if defined(_M_X64)
    /* pushfq; or qword ptr [rsp],0x100; popfq; nop; ret
       The architectural #DB must be delivered after NOP, with RIP at RET. */
    static const unsigned char code[] = {
        0x9c, 0x48, 0x81, 0x0c, 0x24, 0x00, 0x01, 0x00, 0x00,
        0x9d, 0x90, 0xc3
    };
#else
    /* pushfd; or dword ptr [esp],0x100; popfd; nop; ret */
    static const unsigned char code[] = {
        0x9c, 0x81, 0x0c, 0x24, 0x00, 0x01, 0x00, 0x00,
        0x9d, 0x90, 0xc3
    };
#endif
    void* memory = VirtualAlloc(0, sizeof(code), MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE);
    void* handler = AddVectoredExceptionHandler(1, OnException);
    if (!memory || !handler)
        return 2;
    memcpy(memory, code, sizeof(code));
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));
    g_expected_ip = (ULONG_PTR)memory + sizeof(code) - 1;

    ((void (WINAPI*)(void))memory)();

    RemoveVectoredExceptionHandler(handler);
    VirtualFree(memory, 0, MEM_RELEASE);
    printf("single_step_count=%ld\n", g_single_steps);
    if (argc == 2)
    {
        const unsigned long wait_ms = strtoul(argv[1], 0, 10);
        if (wait_ms > 0 && wait_ms <= 60000)
            Sleep((DWORD)wait_ms);
    }
    return g_single_steps == 1 ? 0 : 1;
}
