#include <windows.h>

__declspec(dllexport) int lifecycle_probe(void)
{
    return 0x51A7;
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)instance;
    (void)reason;
    (void)reserved;
    return TRUE;
}
