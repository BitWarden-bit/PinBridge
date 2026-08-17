#include <stdio.h>
#include <windows.h>

__declspec(dllexport) __declspec(noinline) int trap_target(int value)
{
    return value + 7;
}

int main(void)
{
    DWORD waited = 0;
    while (GetFileAttributesA("execution_trap.ready") == INVALID_FILE_ATTRIBUTES &&
           waited < 30000)
    {
        Sleep(20);
        waited += 20;
    }
    if (waited >= 30000)
    {
        fputs("execution trap plugin timeout\n", stderr);
        return 2;
    }
    const int result = trap_target(35);
    printf("trap_target=%d\n", result);
    return result == 42 ? 0 : 3;
}
