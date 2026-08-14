#include "symbols_backend.h"

#include <cstring>

namespace
{

PbStatus CopyString(
    const char* value, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(std::strlen(value)) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value, static_cast<size_t>(*required_size));
    return PB_OK;
}

} // namespace

PbStatus PbBackendUndecorateSymbolName(
    const char*, PbUndecoration style,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(
        style == PB_UNDECORATION_NAME_ONLY ? "PbSymbolFixture" :
        "int __cdecl PbSymbolFixture(int)",
        buffer, capacity, required_size);
}

uint64_t PbBackendSymAddress(PbSymHandle)
{
    return UINT64_C(0x140001000);
}

uint8_t PbBackendSymDynamic(PbSymHandle)
{
    return 1;
}

uint8_t PbBackendSymGeneratedByPin(PbSymHandle)
{
    return 0;
}

uint32_t PbBackendSymIndex(PbSymHandle)
{
    return 7;
}

int32_t PbBackendSymInvalid(void)
{
    return 0;
}

PbStatus PbBackendSymName(
    PbSymHandle, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString("mock_symbol", buffer, capacity, required_size);
}

int32_t PbBackendSymNext(PbSymHandle)
{
    return 92;
}

int32_t PbBackendSymPrev(PbSymHandle)
{
    return 90;
}

uint8_t PbBackendSymValid(PbSymHandle symbol)
{
    return symbol.opaque > 0 ? 1u : 0u;
}

uint64_t PbBackendSymValue(PbSymHandle)
{
    return UINT64_C(0x1000);
}
