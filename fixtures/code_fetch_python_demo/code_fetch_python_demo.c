#include <windows.h>
#include <stdio.h>

#pragma optimize("", off)

__declspec(dllexport) __declspec(noinline) int OriginalFunction(void)
{
    return 1;
}

__declspec(dllexport) __declspec(noinline) int ReplacementFunction(void)
{
    return 2;
}

#pragma optimize("", on)

int main(void)
{
    /* Warm the original routine so the Python update must invalidate Pin's
       existing translation before the second call. */
    int before = OriginalFunction();
    {
        HANDLE ready = CreateFileA(
            "code_fetch_warm.ready", GENERIC_WRITE, FILE_SHARE_READ,
            NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (ready == INVALID_HANDLE_VALUE)
            return 8;
        CloseHandle(ready);
    }
    Sleep(4000);
    int after = OriginalFunction();
    printf("code_fetch_python_demo: before=%d after=%d\n", before, after);
    fflush(stdout);
    Sleep(1000);
    return before == 1 && after == 2 ? 0 : 9;
}
