#include <windows.h>
#include <stdio.h>

static DWORD WINAPI worker_main(void *argument)
{
    (void)argument;
    Sleep(250);
    return 37;
}

int main(void)
{
    /* Leave enough time for the control plane to load the Python plugin. */
    Sleep(4000);

    HMODULE module = LoadLibraryW(L"lifecycle_module_x64.dll");
    if (module == NULL) {
        return 4;
    }
    int (*probe)(void) = (int (*)(void))GetProcAddress(module, "lifecycle_probe");
    if (probe == NULL || probe() != 0x51A7) {
        FreeLibrary(module);
        return 5;
    }
    /* Give the scripting thread a deterministic module-load delivery window. */
    Sleep(750);
    if (!FreeLibrary(module)) {
        return 6;
    }
    /* The unload event must be drained before the process exit sequence. */
    Sleep(750);

    HANDLE worker = CreateThread(NULL, 0, worker_main, NULL, 0, NULL);
    if (worker == NULL) {
        return 2;
    }
    WaitForSingleObject(worker, INFINITE);
    DWORD worker_code = 0;
    GetExitCodeThread(worker, &worker_code);
    CloseHandle(worker);

    /* Let the scripting thread drain the thread-exit edge. */
    Sleep(750);
    printf("lifecycle_demo: worker_exit=%lu\n", (unsigned long)worker_code);
    return worker_code == 37 ? 0 : 3;
}
