#include <windows.h>
#include <stdio.h>

__declspec(dllexport) __declspec(noinline) int IncludedFunction(int value)
{
    volatile int result = value * 3;
    if (result > 0)
        result += 5;
    return result;
}

__declspec(dllexport) __declspec(noinline) int ExcludedFunction(int value)
{
    volatile int result = value + 1;
    return result;
}

int main(void)
{
    /* Warm the included routine before Python changes the policy. The second
       call is observable only if the agent invalidates and re-instruments it. */
    int warm = IncludedFunction(2);
    Sleep(4000);
    int excluded = ExcludedFunction(10);
    int included = IncludedFunction(7);
    printf("instrumentation_python_demo: warm=%d excluded=%d included=%d\n",
           warm, excluded, included);
    fflush(stdout);
    Sleep(1000);
    return warm == 11 && excluded == 11 && included == 26 ? 0 : 7;
}
