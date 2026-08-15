#include <windows.h>
#include <stdio.h>

__declspec(dllexport) volatile unsigned __int64 SourceValue = 0x1111111111111111ULL;
__declspec(dllexport) volatile unsigned __int64 BackingValue = 0x2222222222222222ULL;

__declspec(dllexport) __declspec(noinline) unsigned __int64 ReadMappedSource(void)
{
    return SourceValue;
}

__declspec(noinline) unsigned __int64 ReadPhysicalSource(void)
{
    /* Atomic RMW is classified as STORE by the translation primitive. The
       load-only Python rule must therefore leave this access on SourceValue. */
    return (unsigned __int64)InterlockedCompareExchange64(
        (volatile LONG64 *)&SourceValue, 0, 0);
}

__declspec(noinline) void WritePhysicalSource(unsigned __int64 value)
{
    SourceValue = value;
}

int main(void)
{
    Sleep(4000);
    WritePhysicalSource(0x3333333333333333ULL);
    unsigned __int64 mapped = ReadMappedSource();
    unsigned __int64 physical = ReadPhysicalSource();
    printf("memory_translation_python_demo: mapped=0x%llx physical=0x%llx\n",
           mapped, physical);
    fflush(stdout);
    Sleep(1000);
    return mapped == 0x2222222222222222ULL &&
           physical == 0x3333333333333333ULL ? 0 : 9;
}
