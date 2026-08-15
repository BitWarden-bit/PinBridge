#include <windows.h>
#include <stdio.h>

static volatile LONG g_skip_calls;
static volatile LONG g_return_calls;

__declspec(noinline) __declspec(dllexport)
int DemoSkip(int value)
{
    InterlockedIncrement(&g_skip_calls);
    return value + 10;
}

__declspec(noinline) __declspec(dllexport)
int DemoReturn(int value)
{
    InterlockedIncrement(&g_return_calls);
    return value + 10;
}

int main(void)
{
    typedef int (*DemoApiFn)(int);
    volatile DemoApiFn skip_api = DemoSkip;
    volatile DemoApiFn return_api = DemoReturn;

    /* Give the runner time to load the Python interceptor. */
    Sleep(4000);
    const int skipped = skip_api(5);
    const int returned = return_api(7);
    const LONG skip_calls = g_skip_calls;
    const LONG return_calls = g_return_calls;
    printf("hook_python_demo: intercepted=%d/%d calls=%ld/%ld\n",
           skipped, returned, skip_calls, return_calls);
    return skipped == 0x1234 && returned == 0x5678 &&
           skip_calls == 0 && return_calls == 1 ? 0 : 7;
}
