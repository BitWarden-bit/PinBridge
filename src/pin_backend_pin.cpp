#include "pin.H"

#include "pin_backend.h"
#include "reg_mapping_pin.h"

#include <cstdlib>
#include <cstring>

namespace
{

static_assert(sizeof(INS) == sizeof(int32_t), "Pin 3.31 INS no longer fits PbInsHandle");
static_assert(sizeof(PIN_CALLBACK) <= sizeof(uint64_t), "PIN_CALLBACK no longer fits PbCallbackHandle");

struct InsCallbackState
{
    PbInsInstrumentCallback callback;
    void* user_data;
};

INS ToPinIns(int32_t value)
{
    INS ins;
    ins.q_set(value);
    return ins;
}

VOID OnInstruction(INS ins, VOID* raw_state)
{
    InsCallbackState* state = static_cast<InsCallbackState*>(raw_state);
    PbInsHandle handle      = {ins.q()};
    state->callback(handle, state->user_data);
}

const char* Version() { return PIN_Version().c_str(); }

int32_t Init(int32_t argc, char** argv)
{
    return PIN_Init(static_cast<INT32>(argc), reinterpret_cast<CHAR**>(argv)) ? 1 : 0;
}

void StartProgramDefault() { PIN_StartProgram(); }

PbStatus AddInsInstrumentFunction(PbInsInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    InsCallbackState* state = static_cast<InsCallbackState*>(std::malloc(sizeof(InsCallbackState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback  = callback;
    state->user_data = user_data;

    PIN_CALLBACK pin_callback = INS_AddInstrumentFunction(OnInstruction, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}

uint64_t InsAddress(int32_t ins) { return static_cast<uint64_t>(INS_Address(ToPinIns(ins))); }
uint64_t InsSize(int32_t ins) { return static_cast<uint64_t>(INS_Size(ToPinIns(ins))); }

uint64_t GetContextReg(const void* context, uint32_t reg)
{
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg))
        return 0;
    return static_cast<uint64_t>(
        PIN_GetContextReg(static_cast<const CONTEXT*>(context), native_reg));
}

uint64_t SafeCopy(void* destination, uint64_t source_address, uint64_t size)
{
    return static_cast<uint64_t>(
        PIN_SafeCopy(destination,
                     reinterpret_cast<const VOID*>(static_cast<uintptr_t>(source_address)),
                     static_cast<size_t>(size)));
}

uint64_t SafeCopyEx(
    void* destination, uint64_t source_address, uint64_t size,
    PbExceptionInfoSnapshot* out_exception)
{
    EXCEPTION_INFO exception_info;
    const size_t copied = PIN_SafeCopyEx(
        destination, reinterpret_cast<const VOID*>(static_cast<uintptr_t>(source_address)),
        static_cast<size_t>(size), &exception_info);
    std::memset(out_exception, 0, sizeof(*out_exception));
    if (copied == static_cast<size_t>(size))
        return static_cast<uint64_t>(copied);

    const EXCEPTION_CODE code = PIN_GetExceptionCode(&exception_info);
    const EXCEPTION_CLASS exception_class = PIN_GetExceptionClass(code);
    out_exception->exception_code = static_cast<uint32_t>(code);
    out_exception->exception_class = static_cast<uint32_t>(exception_class);
    out_exception->exception_address = static_cast<uint64_t>(
        PIN_GetExceptionAddress(&exception_info));

    if (exception_class == EXCEPTCLASS_ACCESS_FAULT)
    {
        ADDRINT faulty_address = 0;
        out_exception->faulty_access_type = static_cast<uint32_t>(
            PIN_GetFaultyAccessType(&exception_info));
        if (PIN_GetFaultyAccessAddress(&exception_info, &faulty_address))
        {
            out_exception->flags |= PB_EXCEPTION_INFO_HAS_FAULT_ADDRESS;
            out_exception->faulty_access_address = static_cast<uint64_t>(faulty_address);
        }
    }
    if (exception_class == EXCEPTCLASS_MULTIPLE_FP_ERROR)
    {
        out_exception->flags |= PB_EXCEPTION_INFO_HAS_FP_ERRORS;
        out_exception->fp_errors = PIN_GetFpErrorSet(&exception_info);
    }
    if (code == EXCEPTCODE_WINDOWS)
    {
        out_exception->flags |= PB_EXCEPTION_INFO_HAS_WINDOWS_DETAILS;
        out_exception->windows_exception_code = PIN_GetWindowsExceptionCode(&exception_info);
        UINT32 count = PIN_CountWindowsExceptionArguments(&exception_info);
        if (count > 5u)
            count = 5u;
        out_exception->windows_argument_count = count;
        for (UINT32 index = 0; index < count; ++index)
            out_exception->windows_arguments[index] = static_cast<uint64_t>(
                PIN_GetWindowsExceptionArgument(&exception_info, index));
    }
    return static_cast<uint64_t>(copied);
}

const PbBackend kBackend = {
    Version,
    Init,
    StartProgramDefault,
    AddInsInstrumentFunction,
    InsAddress,
    InsSize,
    GetContextReg,
    SafeCopy,
    SafeCopyEx,
};

} // namespace

const PbBackend& PbGetBackend(void) { return kBackend; }
