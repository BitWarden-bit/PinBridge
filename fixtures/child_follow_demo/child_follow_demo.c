#include <windows.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char** argv)
{
    if (argc >= 2 && strcmp(argv[1], "--child") == 0) {
        const DWORD process_id = GetCurrentProcessId();
        char follow[2] = {0};
        if (GetEnvironmentVariableA("PINBRIDGE_TEST_FOLLOW_CHILD", follow,
                (DWORD)sizeof(follow)) != 0 && follow[0] == '1') {
            char ready_file[64];
            _snprintf_s(ready_file, sizeof(ready_file), _TRUNCATE,
                "child_control_%lu.ready", (unsigned long)process_id);
            const ULONGLONG deadline = GetTickCount64() + 15000;
            while (GetFileAttributesA(ready_file) == INVALID_FILE_ATTRIBUTES &&
                   GetTickCount64() < deadline) {
                Sleep(10);
            }
            if (GetFileAttributesA(ready_file) == INVALID_FILE_ATTRIBUTES)
                return 8;
        }
        printf("child_follow_demo: child pid=%lu token=%s\n",
            (unsigned long)process_id, argc >= 3 ? argv[2] : "missing");
        return 0;
    }

    /* Let the runner load the interceptor before CreateProcess. */
    Sleep(4000);
    char executable[MAX_PATH];
    if (GetModuleFileNameA(NULL, executable, MAX_PATH) == 0)
        return 2;
    char command_line[MAX_PATH + 64];
    _snprintf_s(command_line, sizeof(command_line), _TRUNCATE,
        "\"%s\" --child python-decision", executable);

    STARTUPINFOA startup;
    PROCESS_INFORMATION process;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process, sizeof(process));
    startup.cb = sizeof(startup);
    if (!CreateProcessA(NULL, command_line, NULL, NULL, TRUE, 0, NULL, NULL,
            &startup, &process)) {
        return 3;
    }
    if (WaitForSingleObject(process.hProcess, 20000) == WAIT_TIMEOUT) {
        TerminateProcess(process.hProcess, 9);
        WaitForSingleObject(process.hProcess, 5000);
    }
    DWORD child_exit = 999;
    GetExitCodeProcess(process.hProcess, &child_exit);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    printf("child_follow_demo: parent child_exit=%lu\n", (unsigned long)child_exit);
    return 0;
}
