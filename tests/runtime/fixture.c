#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stddef.h>
#if defined(_MSC_VER)
#include <immintrin.h>
#endif

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

static uint64_t Mix(uint64_t value)
{
    return (value ^ UINT64_C(0x9E3779B97F4A7C15)) + UINT64_C(0x100000001B3);
}

#if defined(_WIN32)
#define PB_FIXTURE_EXPORT __declspec(dllexport) __declspec(noinline)
#else
#define PB_FIXTURE_EXPORT __attribute__((visibility("default"), noinline))
#endif

static volatile uint64_t g_call_application_void_value;

PB_FIXTURE_EXPORT void PbCallApplicationVoid0(void)
{
    g_call_application_void_value = UINT64_C(0x1020304050607080);
}

PB_FIXTURE_EXPORT uint64_t PbCallApplicationU640(void)
{
    return g_call_application_void_value;
}

PB_FIXTURE_EXPORT uint64_t PbCallApplicationU641(uint64_t value)
{
    return value ^ UINT64_C(0x9e3779b97f4a7c15);
}

PB_FIXTURE_EXPORT uint64_t PbCallApplicationU642(uint64_t left, uint64_t right)
{
    return (left ^ UINT64_C(0xd6e8feb86659fd93)) + right;
}

PB_FIXTURE_EXPORT void* PbCallApplicationPtrUsize(size_t size)
{
    return (void*)(uintptr_t)(UINT64_C(0x12345000) + (uint64_t)size);
}

PB_FIXTURE_EXPORT uint64_t PbTraceVersionTarget(uint64_t limit)
{
    volatile uint64_t value = UINT64_C(0x1234);
    uint64_t index;
    for (index = 0; index < limit; ++index)
    {
        if ((index & 1u) != 0)
            value = (value + index) ^ UINT64_C(0x9e3779b97f4a7c15);
        else
            value = (value ^ index) + UINT64_C(0x100000001b3);
    }
    return value;
}

PB_FIXTURE_EXPORT uint64_t PbSyscallReplayTarget(uint64_t value)
{
    return value ^ UINT64_C(0x56c011ab1e);
}

PB_FIXTURE_EXPORT uint64_t PbRtnReplaceJitTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x10);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnReplaceProbeTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x20);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnReplaceProbeExTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result ^= UINT64_C(0x55);
    result += UINT64_C(0x7);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnInsertJitTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x1);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnInsertProbeTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x2);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnInsertProbeExTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x3);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnSignatureJitTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x1010);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnSignatureProbeTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x2020);
    return result;
}

PB_FIXTURE_EXPORT uint64_t PbRtnSignatureProbeExTarget(uint64_t value)
{
    volatile uint64_t result = value;
    result += UINT64_C(0x3030);
    return result;
}

static int RunRtnReplacementFixture(void)
{
    const uint64_t jit = PbRtnReplaceJitTarget(UINT64_C(0x11));
    const uint64_t probe = PbRtnReplaceProbeTarget(UINT64_C(0x22));
    const uint64_t probe_ex = PbRtnReplaceProbeExTarget(UINT64_C(0x33));
    const int jit_mode =
        jit == UINT64_C(0x111) && probe == UINT64_C(0x42) &&
        probe_ex == UINT64_C(0x6d);
    const int probe_mode =
        jit == UINT64_C(0x21) && probe == UINT64_C(0x242) &&
        probe_ex == UINT64_C(0x36d);
    return jit_mode || probe_mode ? 0 : 32;
}

static int RunRtnVarargsFixture(void)
{
    const uint64_t insert_jit = PbRtnInsertJitTarget(UINT64_C(0x1));
    const uint64_t insert_probe = PbRtnInsertProbeTarget(UINT64_C(0x2));
    const uint64_t insert_probe_ex = PbRtnInsertProbeExTarget(UINT64_C(0x3));
    const uint64_t signature_jit = PbRtnSignatureJitTarget(UINT64_C(0x11));
    const uint64_t signature_probe = PbRtnSignatureProbeTarget(UINT64_C(0x22));
    const uint64_t signature_probe_ex =
        PbRtnSignatureProbeExTarget(UINT64_C(0x33));
    const int common =
        insert_jit == UINT64_C(0x2) && insert_probe == UINT64_C(0x4) &&
        insert_probe_ex == UINT64_C(0x6);
    const int jit_mode = common && signature_jit == UINT64_C(0x111) &&
        signature_probe == UINT64_C(0x2042) &&
        signature_probe_ex == UINT64_C(0x3063);
    const int probe_mode = common && signature_jit == UINT64_C(0x1021) &&
        signature_probe == UINT64_C(0x2242) &&
        signature_probe_ex == UINT64_C(0x3363);
    return jit_mode || probe_mode ? 0 : 33;
}

#if defined(_WIN32)
#pragma section(".pbprobe", execute, read)
#pragma comment(linker, "/merge:.pbprobe=.text")
__declspec(allocate(".pbprobe")) __declspec(dllexport)
const unsigned char PbProbeTarget[] = {
    0x48, 0xb8, 0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0xc3
};

__declspec(allocate(".pbprobe")) __declspec(dllexport)
const unsigned char PbInsModificationDeleteTarget[] = {
    0xb8, 0x03, 0x00, 0x00, 0x00, 0x90, 0xc3
};

__declspec(allocate(".pbprobe")) __declspec(dllexport)
const unsigned char PbInsModificationDirectTarget[] = {
    0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3,
    0xb8, 0x07, 0x00, 0x00, 0x00, 0xc3
};

__declspec(allocate(".pbprobe")) __declspec(dllexport)
const unsigned char PbInsModificationIndirectTarget[] = {
    0xb8, 0x02, 0x00, 0x00, 0x00, 0xc3,
    0xb8, 0x08, 0x00, 0x00, 0x00, 0xc3
};

__declspec(allocate(".pbprobe")) __declspec(dllexport)
const unsigned char PbInsModificationMemoryTarget[] = {
    0x48, 0x8b, 0x01, 0xc3
};
#else
__attribute__((visibility("default"), noinline))
void PbProbeTarget(void)
{
    volatile uint64_t value = UINT64_C(0x12345678);
    value ^= UINT64_C(0xabcdef);
}
#endif

static void CallPbProbeTarget(void)
{
#if defined(_WIN32)
    ((void (*)(void))(uintptr_t)PbProbeTarget)();
#else
    PbProbeTarget();
#endif
}

#if defined(_WIN32)
static int TriggerImageUnload(void)
{
    char module_path[MAX_PATH];
    char* filename;
    HMODULE module;

    if (GetModuleFileNameA(0, module_path, MAX_PATH) == 0)
        return 25;
    filename = strrchr(module_path, '\\');
    if (!filename)
        return 26;
    ++filename;
    if (snprintf(
            filename, (size_t)(module_path + MAX_PATH - filename),
            "pinbridge_mock.dll") < 0)
        return 27;
    module = LoadLibraryA(module_path);
    if (!module)
        return 28;
    Sleep(100);
    if (!FreeLibrary(module))
        return 29;
    Sleep(100);
    return 0;
}
#endif

#if defined(_WIN32)
PB_FIXTURE_EXPORT int PbInsModificationGatherTarget(const int* values)
{
    const __m256i indices = _mm256_setr_epi32(0, 1, 2, 3, 4, 5, 6, 7);
    const __m256i gathered = _mm256_i32gather_epi32(values, indices, 4);
    int result[8];
    int sum = 0;
    int index;
    _mm256_storeu_si256((__m256i*)result, gathered);
    for (index = 0; index < 8; ++index)
        sum += result[index];
    return sum;
}

static int RunInsModificationFixture(void)
{
    typedef int (*NoArgFunction)(void);
    typedef int (*IndirectFunction)(uintptr_t);
    typedef uint64_t (*MemoryFunction)(const uint64_t*);
    static const int gather_values[8] = {1, 2, 3, 4, 5, 6, 7, 8};
    const uint64_t original_memory = UINT64_C(0x1111111111111111);
    const int deleted = ((NoArgFunction)(uintptr_t)PbInsModificationDeleteTarget)();
    const int direct = ((NoArgFunction)(uintptr_t)PbInsModificationDirectTarget)();
    const uintptr_t indirect_target =
        (uintptr_t)PbInsModificationIndirectTarget + 6u;
    const int indirect =
        ((IndirectFunction)(uintptr_t)PbInsModificationIndirectTarget)(indirect_target);
    const uint64_t memory =
        ((MemoryFunction)(uintptr_t)PbInsModificationMemoryTarget)(&original_memory);
    const int gathered = PbInsModificationGatherTarget(gather_values);

    return deleted == 3 && direct == 7 && indirect == 8 &&
        memory == UINT64_C(0x2222222222222222) && gathered == 36 ? 0 : 24;
}
#endif

static void TriggerHandledException(void)
{
#if defined(_WIN32)
    __try
    {
        volatile const uint32_t* invalid_address =
            (volatile const uint32_t*)(uintptr_t)1u;
        volatile uint32_t value = *invalid_address;
        (void)value;
    }
    __except (EXCEPTION_EXECUTE_HANDLER)
    {
    }
#endif
}

#if defined(_WIN32)
static unsigned char g_smc_code[64];

static int TriggerSelfModifyingCode(void)
{
    typedef int (*CodeFunction)(void);
    static const unsigned char code_one[] = {
        0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3
    };
    static const unsigned char code_two[] = {
        0xb8, 0x02, 0x00, 0x00, 0x00, 0xc3
    };
    SYSTEM_INFO system_info;
    unsigned char* page;
    DWORD old_protection = 0;
    uint32_t iteration;

    GetSystemInfo(&system_info);
    page = (unsigned char*)((uintptr_t)g_smc_code &
        ~((uintptr_t)system_info.dwPageSize - 1u));
    if (!VirtualProtect(
            page, system_info.dwPageSize, PAGE_EXECUTE_READWRITE,
            &old_protection))
        return 20;
    for (iteration = 0; iteration < 3; ++iteration)
    {
        memcpy(g_smc_code, code_one, sizeof(code_one));
        FlushInstructionCache(GetCurrentProcess(), g_smc_code, sizeof(code_one));
        if (((CodeFunction)(uintptr_t)g_smc_code)() != 1)
            return 21;
        memcpy(g_smc_code, code_two, sizeof(code_two));
        FlushInstructionCache(GetCurrentProcess(), g_smc_code, sizeof(code_two));
        if (((CodeFunction)(uintptr_t)g_smc_code)() != 2)
            return 22;
    }
    return 0;
}
#endif

#if defined(_WIN32)
static int SpawnChildAndWait(void)
{
    char executable[MAX_PATH];
    char command_line[(MAX_PATH * 2) + 64];
    STARTUPINFOA startup;
    PROCESS_INFORMATION process;
    DWORD exit_code = 1;

    if (GetModuleFileNameA(0, executable, MAX_PATH) == 0)
        return 10;
    if (snprintf(command_line, sizeof(command_line),
            "\"%s\" --pinbridge-child --payload", executable) < 0)
        return 11;
    memset(&startup, 0, sizeof(startup));
    memset(&process, 0, sizeof(process));
    startup.cb = sizeof(startup);
    if (!CreateProcessA(executable, command_line, 0, 0, FALSE, 0, 0, 0,
            &startup, &process))
        return 12;
    WaitForSingleObject(process.hProcess, INFINITE);
    if (!GetExitCodeProcess(process.hProcess, &exit_code))
        exit_code = 13;
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return (int)exit_code;
}
#endif

int main(int argc, char** argv)
{
    volatile uint64_t value = 1;
    uint32_t index;
#if defined(_WIN32)
    if (argc > 1 && strcmp(argv[1], "--pinbridge-spawn-child") == 0)
    {
        const int child_result = SpawnChildAndWait();
        if (child_result != 0)
            return child_result;
    }
    if (argc > 1 && strcmp(argv[1], "--pinbridge-detach-wait") == 0)
        Sleep(2000);
    if (argc > 1 && strcmp(argv[1], "--pinbridge-smc") == 0)
    {
        const int smc_result = TriggerSelfModifyingCode();
        if (smc_result != 0)
            return smc_result;
    }
    if (argc > 1 && strcmp(argv[1], "--pinbridge-trace-version") == 0)
    {
        volatile uint64_t version_result = PbTraceVersionTarget(UINT64_C(10000));
        if (version_result == 0)
            return 23;
    }
    if (argc > 1 && strcmp(argv[1], "--pinbridge-ins-modification") == 0)
    {
        const int modification_result = RunInsModificationFixture();
        if (modification_result != 0)
            return modification_result;
    }
    if (argc > 1 && strcmp(argv[1], "--pinbridge-img-unload") == 0)
    {
        const int image_result = TriggerImageUnload();
        if (image_result != 0)
            return image_result;
        const int replacement_result = RunRtnReplacementFixture();
        if (replacement_result != 0)
            return replacement_result;
        const int varargs_result = RunRtnVarargsFixture();
        if (varargs_result != 0)
            return varargs_result;
    }
    if (argc > 1 && strcmp(argv[1], "--pinbridge-syscall") == 0)
    {
        HANDLE file = CreateFileA(
            "NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
            0, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, 0);
        volatile uint64_t syscall_result;
        if (file == INVALID_HANDLE_VALUE)
            return 30;
        CloseHandle(file);
        syscall_result = PbSyscallReplayTarget(UINT64_C(0x12345678));
        if (syscall_result == 0)
            return 31;
    }
#else
    (void)argc;
    (void)argv;
#endif
    TriggerHandledException();
    CallPbProbeTarget();
    for (index = 0; index < 10000; ++index)
        value = Mix(value + index);
    return value == 0 ? 1 : 0;
}
