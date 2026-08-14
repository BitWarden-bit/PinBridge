#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

#pragma warning(disable : 4191)

typedef uint32_t(PB_CALL* AbiVersionFn)(void);
typedef PbStatus(PB_CALL* PinVersionFn)(char*, uint64_t, uint64_t*);
typedef PbStatus(PB_CALL* PinInitFn)(int32_t, char**);
typedef PbStatus(PB_CALL* AddInsFn)(PbInsInstrumentCallback, void*, PbCallbackHandle*);
typedef PbStatus(PB_CALL* InsAddressFn)(PbInsHandle, uint64_t*);
typedef PbStatus(PB_CALL* InsSizeFn)(PbInsHandle, uint64_t*);
typedef PbStatus(PB_CALL* GetContextRegFn)(PbConstContextHandle, PbRegId, uint64_t*);
typedef PbStatus(PB_CALL* SafeCopyFn)(void*, uint64_t, uint64_t, uint64_t*);

static PbInsHandle g_callback_ins;
static uint32_t g_callback_count;

static void PB_CALL OnInstruction(PbInsHandle ins, void* user_data)
{
    uint32_t* marker = (uint32_t*)user_data;
    g_callback_ins = ins;
    ++g_callback_count;
    ++(*marker);
}

static FARPROC RequireSymbol(HMODULE module, const char* name)
{
    FARPROC symbol = GetProcAddress(module, name);
    if (!symbol)
        fprintf(stderr, "missing export: %s\n", name);
    return symbol;
}

#define CHECK(condition, message)          \
    do                                     \
    {                                      \
        if (!(condition))                  \
        {                                  \
            fprintf(stderr, "%s\n", message); \
            return 1;                      \
        }                                  \
    } while (0)

int main(int argc, char** argv)
{
    HMODULE module;
    AbiVersionFn abi_version;
    PinVersionFn pin_version;
    PinInitFn pin_init;
    AddInsFn add_ins;
    InsAddressFn ins_address;
    InsSizeFn ins_size;
    GetContextRegFn get_context_reg;
    SafeCopyFn safe_copy;
    uint64_t required = 0;
    char version[64];
    char tiny[2] = {'X', '\0'};
    char reject_arg[] = "--reject";
    char* reject_argv[] = {reject_arg};
    uint32_t marker = 0;
    PbCallbackHandle callback_handle = {0};
    uint64_t value = 0;
    uint64_t copied = 0;
    char source[] = "safe-copy";
    char destination[sizeof(source)] = {0};

    CHECK(argc == 2, "usage: pb_contract_loader <bridge.dll>");
    module = LoadLibraryA(argv[1]);
    if (!module)
    {
        fprintf(stderr, "cannot load bridge DLL: %s (error %lu)\n", argv[1], GetLastError());
        return 1;
    }

    abi_version = (AbiVersionFn)RequireSymbol(module, "pb_abi_version");
    pin_version = (PinVersionFn)RequireSymbol(module, "pb_pin_version");
    pin_init = (PinInitFn)RequireSymbol(module, "pb_pin_init");
    add_ins = (AddInsFn)RequireSymbol(module, "pb_ins_add_instrument_function");
    ins_address = (InsAddressFn)RequireSymbol(module, "pb_ins_address");
    ins_size = (InsSizeFn)RequireSymbol(module, "pb_ins_size");
    get_context_reg = (GetContextRegFn)RequireSymbol(module, "pb_pin_get_context_reg");
    safe_copy = (SafeCopyFn)RequireSymbol(module, "pb_pin_safe_copy");
    CHECK(abi_version && pin_version && pin_init && add_ins && ins_address && ins_size &&
              get_context_reg && safe_copy,
          "one or more required exports are missing");

    CHECK(abi_version() == PB_ABI_VERSION, "ABI version mismatch");
    CHECK(pin_version(NULL, 0, &required) == PB_ERR_BUFFER_TOO_SMALL, "version size query failed");
    CHECK(required > 1 && required <= sizeof(version), "invalid required version size");
    CHECK(pin_version(tiny, sizeof(tiny), &required) == PB_ERR_BUFFER_TOO_SMALL,
          "small version buffer was accepted");
    CHECK(tiny[0] == 'X', "small buffer was partially overwritten");
    CHECK(pin_version(version, sizeof(version), &required) == PB_OK, "version copy failed");
    CHECK(strcmp(version, "PinMock 3.31") == 0, "unexpected mock version");

    CHECK(pin_init(-1, NULL) == PB_ERR_INVALID_ARGUMENT, "negative argc was accepted");
    CHECK(pin_init(1, reject_argv) == PB_ERR_PIN_REJECTED_ARGUMENTS,
          "Pin argument rejection was not mapped");
    CHECK(pin_init(0, NULL) == PB_OK, "valid Pin init failed");

    CHECK(add_ins(NULL, NULL, &callback_handle) == PB_ERR_INVALID_ARGUMENT,
          "NULL instrumentation callback was accepted");
    CHECK(add_ins(OnInstruction, &marker, &callback_handle) == PB_OK,
          "instrumentation callback registration failed");
    CHECK(callback_handle.opaque != 0, "callback handle is invalid");
    CHECK(g_callback_count == 1 && marker == 1 && g_callback_ins.opaque == 42,
          "callback trampoline contract failed");

    CHECK(ins_address(g_callback_ins, &value) == PB_OK && value == UINT64_C(0x401000),
          "INS_Address mapping failed");
    CHECK(ins_size(g_callback_ins, &value) == PB_OK && value == 7,
          "INS_Size mapping failed");
    CHECK(get_context_reg(NULL, 1, &value) == PB_ERR_INVALID_ARGUMENT,
          "NULL context was accepted");

    CHECK(safe_copy(destination, (uint64_t)(uintptr_t)source, sizeof(source), &copied) == PB_OK,
          "safe copy call failed");
    CHECK(copied == sizeof(source) && memcmp(source, destination, sizeof(source)) == 0,
          "safe copy content mismatch");
    CHECK(safe_copy(NULL, 0, 1, &copied) == PB_ERR_INVALID_ARGUMENT,
          "NULL safe-copy destination was accepted");

    FreeLibrary(module);
    return 0;
}

