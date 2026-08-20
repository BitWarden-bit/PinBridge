#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static volatile LONG g_single_steps;
static ULONG_PTR g_expected_ip;
static ULONG_PTR g_pushed_flags;

static LONG CALLBACK OnException(EXCEPTION_POINTERS* pointers)
{
    if (pointers->ExceptionRecord->ExceptionCode != EXCEPTION_SINGLE_STEP)
        return EXCEPTION_CONTINUE_SEARCH;
#if defined(_M_X64)
    if (pointers->ContextRecord->Rip != g_expected_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    g_pushed_flags = *(const ULONG_PTR*)pointers->ContextRecord->Rsp;
    pointers->ContextRecord->EFlags &= ~0x100u;
#else
    if (pointers->ContextRecord->Eip != (DWORD)g_expected_ip)
        return EXCEPTION_CONTINUE_SEARCH;
    g_pushed_flags = *(const ULONG_PTR*)pointers->ContextRecord->Esp;
    pointers->ContextRecord->EFlags &= ~0x100u;
#endif
    InterlockedIncrement(&g_single_steps);
    return EXCEPTION_CONTINUE_EXECUTION;
}

int main(int argc, char** argv)
{
#if defined(_M_X64)
    /* Set TF with POPFQ, execute PUSHFQ as the stepped instruction, then
       inspect the pushed image in the #DB handler before POP consumes it. */
    static const unsigned char code[] = {
        0x9c, 0x48, 0x81, 0x0c, 0x24, 0x00, 0x01, 0x00, 0x00,
        0x9d, 0x9c, 0x58, 0xc3
    };
#else
    static const unsigned char code[] = {
        0x9c, 0x81, 0x0c, 0x24, 0x00, 0x01, 0x00, 0x00,
        0x9d, 0x9c, 0x58, 0xc3
    };
#endif
    void* memory = VirtualAlloc(0, sizeof(code), MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE);
    void* handler = AddVectoredExceptionHandler(1, OnException);
    if (!memory || !handler)
        return 2;
    memcpy(memory, code, sizeof(code));
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));
#if defined(_M_X64)
    g_expected_ip = (ULONG_PTR)memory + 11;
#else
    g_expected_ip = (ULONG_PTR)memory + 10;
#endif

    ((void (WINAPI*)(void))memory)();

    RemoveVectoredExceptionHandler(handler);
    VirtualFree(memory, 0, MEM_RELEASE);
    printf("single_step_count=%ld pushed_tf=%lu\n", g_single_steps,
        (unsigned long)((g_pushed_flags >> 8) & 1));
    if (argc == 2)
    {
        const unsigned long wait_ms = strtoul(argv[1], 0, 10);
        if (wait_ms > 0 && wait_ms <= 60000)
            Sleep((DWORD)wait_ms);
    }
    return g_single_steps == 1 && (g_pushed_flags & 0x100) != 0 ? 0 : 1;
}
