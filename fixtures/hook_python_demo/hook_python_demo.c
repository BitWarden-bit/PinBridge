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
    const int skipped_first = skip_api(5);
    const int skipped_second = skip_api(5);
    const int returned_first = return_api(7);
    const int returned_second = return_api(7);
    const LONG skip_calls = g_skip_calls;
    const LONG return_calls = g_return_calls;
    /* Let the asynchronous Hook observers drain before exit preparation. */
    Sleep(750);
    printf("hook_python_demo: skipped=%d/%d returned=%d/%d calls=%ld/%ld\n",
           skipped_first, skipped_second, returned_first, returned_second,
           skip_calls, return_calls);
    return skipped_first == 0x1234 && skipped_second == 0x1234 &&
           returned_first == 0x5678 && returned_second == 17 &&
           skip_calls == 0 && return_calls == 2 ? 0 : 7;
}
