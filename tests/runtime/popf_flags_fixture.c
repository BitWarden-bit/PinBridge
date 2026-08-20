#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void)
{
#if !defined(_M_X64)
    return 77;
#else
    /* Windows x64: RCX=requested flags, RDX=&before, RAX=observed flags.
       pushfq; pop rax; mov [rdx],rax; push rcx; popfq; pushfq; pop rax;
       push qword ptr [rdx]; popfq; ret */
    static const unsigned char code[] = {
        0x9c, 0x58, 0x48, 0x89, 0x02, 0x51, 0x9d,
        0x9c, 0x58, 0xff, 0x32, 0x9d, 0xc3
    };
    typedef uint64_t (WINAPI *FlagsRoutine)(uint64_t, uint64_t*);
    unsigned char* memory = (unsigned char*)VirtualAlloc(0, sizeof(code),
        MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    if (!memory)
        return 2;
    memcpy(memory, code, sizeof(code));
    FlushInstructionCache(GetCurrentProcess(), memory, sizeof(code));

    uint64_t before = 0;
    /* Arithmetic flags and DF are user-writable and safe to toggle across the
       short instruction-only window. Avoid AC/NT because they alter unrelated
       process execution; TF has separate runtime fixtures. */
    const uint64_t user_mask = 0x0cd5;
    const uint64_t protected_mask = 0x3200; /* IF | IOPL */
    const uint64_t requested = user_mask;
    const uint64_t after = ((FlagsRoutine)memory)(requested, &before);
    VirtualFree(memory, 0, MEM_RELEASE);

    const uint64_t expected_user = requested & user_mask;
    const int user_ok = (after & user_mask) == expected_user;
    const int protected_ok = (after & protected_mask) ==
        (before & protected_mask);
    const int reserved_ok = (after & 2) != 0;
    const int tf_ok = (after & 0x100) == 0;
    printf("before=%llx after=%llx user_ok=%d protected_ok=%d reserved_ok=%d tf_ok=%d\n",
        (unsigned long long)before, (unsigned long long)after, user_ok,
        protected_ok, reserved_ok, tf_ok);
    return user_ok && protected_ok && reserved_ok && tf_ok ? 0 : 1;
#endif
}
