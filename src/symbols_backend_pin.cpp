#include "pin.H"

#include "symbols_backend.h"

#include <cstring>
#include <string>

namespace
{

static_assert(sizeof(SYM) == sizeof(int32_t), "Pin 3.31 SYM layout changed");
static_assert(PB_UNDECORATION_COMPLETE == UNDECORATION_COMPLETE,
              "UNDECORATION_COMPLETE value changed");
static_assert(PB_UNDECORATION_NAME_ONLY == UNDECORATION_NAME_ONLY,
              "UNDECORATION_NAME_ONLY value changed");

SYM ToSym(PbSymHandle symbol)
{
    SYM result;
    result.q_set(symbol.opaque);
    return result;
}

PbStatus CopyString(
    const std::string& value, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(value.size()) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}

} // namespace

PbStatus PbBackendUndecorateSymbolName(
    const char* symbol_name, PbUndecoration style,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(PIN_UndecorateSymbolName(
        std::string(symbol_name), static_cast<UNDECORATION>(style)),
        buffer, capacity, required_size);
}

uint64_t PbBackendSymAddress(PbSymHandle symbol)
{
    return static_cast<uint64_t>(SYM_Address(ToSym(symbol)));
}

uint8_t PbBackendSymDynamic(PbSymHandle symbol)
{
    return SYM_Dynamic(ToSym(symbol)) ? 1u : 0u;
}

uint8_t PbBackendSymGeneratedByPin(PbSymHandle symbol)
{
    return SYM_GeneratedByPin(ToSym(symbol)) ? 1u : 0u;
}

uint32_t PbBackendSymIndex(PbSymHandle symbol)
{
    return static_cast<uint32_t>(SYM_Index(ToSym(symbol)));
}

int32_t PbBackendSymInvalid(void)
{
    return SYM_Invalid().q();
}

PbStatus PbBackendSymName(
    PbSymHandle symbol, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(
        SYM_Name(ToSym(symbol)), buffer, capacity, required_size);
}

int32_t PbBackendSymNext(PbSymHandle symbol)
{
    return SYM_Next(ToSym(symbol)).q();
}

int32_t PbBackendSymPrev(PbSymHandle symbol)
{
    return SYM_Prev(ToSym(symbol)).q();
}

uint8_t PbBackendSymValid(PbSymHandle symbol)
{
    return SYM_Valid(ToSym(symbol)) ? 1u : 0u;
}

uint64_t PbBackendSymValue(PbSymHandle symbol)
{
    return static_cast<uint64_t>(SYM_Value(ToSym(symbol)));
}
