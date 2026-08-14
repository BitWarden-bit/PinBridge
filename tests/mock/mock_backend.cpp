#include "pin_backend.h"

#include <cstring>

namespace
{

const char* Version() { return "PinMock 3.31"; }

int32_t Init(int32_t argc, char** argv)
{
    return argc == 1 && argv && std::strcmp(argv[0], "--reject") == 0;
}

void StartProgramDefault() {}

PbStatus AddInsInstrumentFunction(PbInsInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    PbInsHandle ins = {42};
    *out_callback = UINT64_C(0x1234);
    callback(ins, user_data);
    return PB_OK;
}

uint64_t InsAddress(int32_t ins) { return ins == 42 ? UINT64_C(0x401000) : 0; }
uint64_t InsSize(int32_t ins) { return ins == 42 ? 7u : 0u; }

uint64_t GetContextReg(const void* context, uint32_t reg)
{
    return *static_cast<const uint64_t*>(context) + reg;
}

uint64_t SafeCopy(void* destination, uint64_t source_address, uint64_t size)
{
    if (size != 0)
        std::memcpy(destination, reinterpret_cast<const void*>(static_cast<uintptr_t>(source_address)),
                    static_cast<size_t>(size));
    return size;
}

uint64_t SafeCopyEx(
    void* destination, uint64_t source_address, uint64_t size,
    PbExceptionInfoSnapshot* out_exception)
{
    std::memset(out_exception, 0, sizeof(*out_exception));
    if (size != 0 && source_address == 0)
    {
        out_exception->exception_code = 1u;
        out_exception->exception_class = 1u;
        out_exception->flags = PB_EXCEPTION_INFO_HAS_FAULT_ADDRESS;
        out_exception->faulty_access_type = 1u;
        return 0;
    }
    return SafeCopy(destination, source_address, size);
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

const PbBackend& PbGetBackend(void)
{
    return kBackend;
}
