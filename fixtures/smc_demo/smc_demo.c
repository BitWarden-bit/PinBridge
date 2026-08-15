#include <windows.h>
#include <stdio.h>
#include <string.h>

typedef int (__cdecl *GeneratedFunction)(void);

int main(void)
{
    /* Give the control plane time to load the Python subscription before the
       first translation of the generated function. */
    Sleep(4000);

    unsigned char code[] = {0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3};
    unsigned char* memory = (unsigned char*)VirtualAlloc(
        NULL, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    if (memory == NULL)
        return 2;
    memcpy(memory, code, sizeof(code));
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));

    GeneratedFunction generated = (GeneratedFunction)memory;
    int before = generated();

    /* Change `mov eax, 1` to `mov eax, 2` after Pin translated the trace. */
    memory[1] = 0x02;
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));
    int after = generated();

    Sleep(1000);
    printf("smc_demo: before=%d after=%d\n", before, after);
    VirtualFree(memory, 0, MEM_RELEASE);
    return before == 1 && after == 2 ? 0 : 3;
}
