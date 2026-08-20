#include <windows.h>
#include <stdio.h>
#include <setjmp.h>

#define UI_DEMO_EXCEPTION EXCEPTION_ACCESS_VIOLATION

static jmp_buf g_recovery;

/* The UI-created takeover callback resolves this symbol and redirects the
 * exception destination here. It deliberately never returns to the synthetic
 * call frame: longjmp restores the loop's real C context. */
__declspec(dllexport) __declspec(noinline) void RecoveryPoint(void)
{
    longjmp(g_recovery, 1);
}

int main(void)
{
    unsigned long count = 0;
    SetConsoleTitleA("PinBridge exception UI demo");
    Sleep(2500);
    for (;;) {
        if (setjmp(g_recovery) != 0) {
            ++count;
            printf("exception_ui_demo: callback takeover #%lu\n", count);
            fflush(stdout);
        } else {
            __try {
                volatile int *bad = (volatile int *)(UINT_PTR)1;
                *bad = (int)count;
            } __except (GetExceptionCode() == UI_DEMO_EXCEPTION
                            ? EXCEPTION_EXECUTE_HANDLER
                            : EXCEPTION_CONTINUE_SEARCH) {
                ++count;
                printf("exception_ui_demo: native handler #%lu\n", count);
                fflush(stdout);
            }
        }
        Sleep(3000);
    }
}
