#include <windows.h>
#include <stdio.h>

__declspec(noinline) __declspec(dllexport)
int DemoApi(int value)
{
    return value + 10;
}

int main(void)
{
    typedef int (*DemoApiFn)(int);
    volatile DemoApiFn api = DemoApi;
    volatile int result = 0;

    puts("function_hook_fixture: ready");
    fflush(stdout);
    for (int index = 0; index < 200; ++index) {
        result = api(5);
        Sleep(50);
    }
    printf("function_hook_fixture: result=%d\n", result);
    fflush(stdout);
    Sleep(10000);
    return result == 15 ? 0 : 1;
}
