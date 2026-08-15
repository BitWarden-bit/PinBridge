#include <windows.h>
#include <stdio.h>

void DecodeTarget(void);

static int Touch(const char* path)
{
    HANDLE file = CreateFileA(path, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file == INVALID_HANDLE_VALUE)
        return 0;
    CloseHandle(file);
    return 1;
}

int main(void)
{
    DWORD waited = 0;
    if (!Touch("xed_decode.ready"))
        return 2;
    while (GetFileAttributesA("xed_decode.go") == INVALID_FILE_ATTRIBUTES && waited < 20000)
    {
        Sleep(25);
        waited += 25;
    }
    if (waited >= 20000)
        return 3;

    __try
    {
        DecodeTarget();
    }
    __except (EXCEPTION_EXECUTE_HANDLER)
    {
        /* Some physical CPUs do not implement CLDEMOTE. Pin decoding has
           already happened, which is what this fixture verifies. */
    }
    puts("xed_decode_python_demo: target decoded");
    fflush(stdout);
    Sleep(1500);
    return 0;
}
