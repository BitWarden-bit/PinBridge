#include <windows.h>
#include <stdio.h>
#include <setjmp.h>

#define PYTHON_EXCEPTION_CODE EXCEPTION_ACCESS_VIOLATION

static jmp_buf g_recovery;

__declspec(dllexport) __declspec(noinline) void RecoveryPoint(void)
{
    longjmp(g_recovery, 1);
}

int main(void)
{
    if (setjmp(g_recovery) != 0) {
        printf("exception_python_demo: RECOVERED\n");
        fflush(stdout);
        /* Let asynchronous Python observers drain the mirrored exception. */
        Sleep(750);
        return 0;
    }
    /* Give the runner time to load the Python interceptor. */
    Sleep(4000);
    __try {
        volatile int *bad = (volatile int *)(UINT_PTR)1;
        *bad = 0x42;
    } __except (GetExceptionCode() == PYTHON_EXCEPTION_CODE
                    ? EXCEPTION_EXECUTE_HANDLER
                    : EXCEPTION_CONTINUE_SEARCH) {
        printf("exception_python_demo: NATIVE_HANDLER\n");
        return 7;
    }
    return 8;
}
