#include "control_fetch_backend.h"

#include <cstring>

PbStatus PbBackendAddFetchFunction(PbFetchCallback callback, void* user_data)
{
    uint8_t buffer[4] = {};
    const uint64_t copied = callback(
        buffer, UINT64_C(0x1000), sizeof(buffer),
        reinterpret_cast<PbExceptionInfoHandle>(static_cast<uintptr_t>(0x8000)), user_data);
    return copied == sizeof(buffer) ? PB_OK : PB_ERR_INTERNAL;
}

uint64_t PbBackendFetchCode(
    void* copy_buffer, uint64_t address, uint64_t max_size, PbExceptionInfoHandle)
{
    static const uint8_t bytes[] = {0x90, 0x90, 0xcc, 0xc3};
    const uint64_t copied = max_size < sizeof(bytes) ? max_size : sizeof(bytes);
    if (copied != 0)
        std::memcpy(copy_buffer, bytes, static_cast<size_t>(copied));
    return address == 0 && max_size != 0 ? 0 : copied;
}

uint64_t PbBackendFetchOriginalCode(
    void* copy_buffer, uint64_t address, uint64_t max_size, PbExceptionInfoHandle)
{
    static const uint8_t bytes[] = {0x55, 0x48, 0x89, 0xe5};
    const uint64_t copied = max_size < sizeof(bytes) ? max_size : sizeof(bytes);
    if (copied != 0 && address != 0)
        std::memcpy(copy_buffer, bytes, static_cast<size_t>(copied));
    return address == 0 ? 0 : copied;
}
