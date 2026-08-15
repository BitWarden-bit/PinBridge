#include <stdio.h>
#include <stdint.h>
#include <windows.h>

static volatile long g_handled;
static volatile long g_veh_seen;

static LONG CALLBACK observe_veh(PEXCEPTION_POINTERS pointers) {
    if (pointers != NULL && pointers->ExceptionRecord != NULL) {
        DWORD code = pointers->ExceptionRecord->ExceptionCode;
        if (code == EXCEPTION_ACCESS_VIOLATION ||
            code == EXCEPTION_BREAKPOINT ||
            code == EXCEPTION_INT_DIVIDE_BY_ZERO) {
            InterlockedIncrement(&g_veh_seen);
        }
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

__declspec(noinline) static int raise_access_violation(void) {
    volatile int *bad = (volatile int *)(uintptr_t)1;
    *bad = 0x42;
    return 0;
}

__declspec(noinline) static int raise_breakpoint(void) {
    __debugbreak();
    return 0;
}

__declspec(noinline) static int raise_divide_by_zero(void) {
    volatile int zero = 0;
    return 123 / zero;
}

__declspec(noinline) static int handle_one(void) {
    __try {
        (void)raise_access_violation();
        return 0;
    } __except (GetExceptionCode() == EXCEPTION_ACCESS_VIOLATION
                    ? EXCEPTION_EXECUTE_HANDLER
                    : EXCEPTION_CONTINUE_SEARCH) {
        InterlockedIncrement(&g_handled);
        return 1;
    }
}

__declspec(noinline) static int handle_breakpoint(void) {
    __try {
        (void)raise_breakpoint();
        return 0;
    } __except (GetExceptionCode() == EXCEPTION_BREAKPOINT
                    ? EXCEPTION_EXECUTE_HANDLER
                    : EXCEPTION_CONTINUE_SEARCH) {
        InterlockedIncrement(&g_handled);
        return 1;
    }
}

__declspec(noinline) static int handle_divide_by_zero(void) {
    __try {
        (void)raise_divide_by_zero();
        return 0;
    } __except (GetExceptionCode() == EXCEPTION_INT_DIVIDE_BY_ZERO
                    ? EXCEPTION_EXECUTE_HANDLER
                    : EXCEPTION_CONTINUE_SEARCH) {
        InterlockedIncrement(&g_handled);
        return 1;
    }
}

int main(void) {
    PVOID veh = AddVectoredExceptionHandler(1, observe_veh);
    if (veh == NULL) {
        printf("exception_demo failed to install VEH\n");
        return 3;
    }
    /* Let the runner connect and silence startup syscall traffic first. */
    Sleep(1000);
    int first = handle_one();
    int breakpoint = handle_breakpoint();
    int divide_by_zero = handle_divide_by_zero();
    printf("exception_demo start av=%d bp=%d div0=%d veh=%ld\n",
           first, breakpoint, divide_by_zero, g_veh_seen);
    fflush(stdout);

    /* Keep the target alive long enough for a control-plane event poll. */
    Sleep(250);
    int second = handle_one();
    printf("exception_demo handled=%d total=%ld veh=%ld\n",
           second, g_handled, g_veh_seen);
    fflush(stdout);
    Sleep(1500);

    RemoveVectoredExceptionHandler(veh);
    return (first == 1 && breakpoint == 1 && divide_by_zero == 1 &&
            second == 1 && g_handled == 4 && g_veh_seen == 4) ? 0 : 2;
}
