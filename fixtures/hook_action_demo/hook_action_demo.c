#include <windows.h>
#include <stdio.h>

/* A deliberately small exported API used by the Hook action smoke test. */
__declspec(noinline) __declspec(dllexport)
int DemoApi(int value)
{
    return value + 10;
}

int main(void)
{
    typedef int (*DemoApiFn)(int);
    volatile DemoApiFn api = DemoApi;
    volatile int input = 5;
    const int baseline = api(input);

    /* Give the controller time to arm both the entry and ret instruction. */
    Sleep(30000);

    const int hooked = api(input);
    /* Keep the process alive long enough for the control plane to read both
       Hook events before Windows teardown syscalls fill the newest window. */
    Sleep(5000);
    printf("hook_action_demo: input=%d baseline=%d hooked=%d\n",
           input, baseline, hooked);
    return hooked == 0x1234 ? 0 : 7;
}
