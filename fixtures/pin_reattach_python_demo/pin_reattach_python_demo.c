#include <stdint.h>
#include <stdio.h>
#include <windows.h>

__declspec(dllexport) volatile LONG ReattachReady = 0;

__declspec(dllexport) __declspec(noinline) uint64_t AfterAttachTarget(uint64_t value)
{
    return (value ^ UINT64_C(0x13579bdf)) + UINT64_C(0x2468ace0);
}

static DWORD WINAPI worker_main(void* argument)
{
    const uint64_t input = (uint64_t)(uintptr_t)argument;
    return (DWORD)(AfterAttachTarget(input) & 0xffu);
}

int main(void)
{
    unsigned waited_ms = 0;
    HANDLE worker;
    DWORD worker_code = 0;

    while (InterlockedCompareExchange(&ReattachReady, 0, 0) == 0 && waited_ms < 5000) {
        Sleep(25);
        waited_ms += 25;
    }

    worker = CreateThread(NULL, 0, worker_main, (void*)(uintptr_t)7, 0, NULL);
    if (worker == NULL)
        return 3;
    WaitForSingleObject(worker, INFINITE);
    GetExitCodeThread(worker, &worker_code);
    CloseHandle(worker);

    /* Leave time for the restored script host to drain instruction/thread events. */
    Sleep(1500);
    printf("pin_reattach_python_demo: wait_ms=%u worker=%lu value=%llu\n",
        waited_ms, (unsigned long)worker_code,
        (unsigned long long)AfterAttachTarget(11));
    return 0;
}
