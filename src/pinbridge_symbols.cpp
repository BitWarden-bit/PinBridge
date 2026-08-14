#include "pinbridge/pinbridge.h"

#include "symbols_backend.h"

namespace
{

template< typename Function > PbStatus GuardSymbols(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsValid(PbSymHandle symbol)
{
    return symbol.opaque > 0;
}

template< typename T, typename Function >
PbStatus QueryScalar(PbSymHandle symbol, T* out_value, Function function)
{
    if (!IsValid(symbol) || !out_value)
        return PB_ERR_INVALID_ARGUMENT;
    *out_value = 0;
    return GuardSymbols([&]() -> PbStatus {
        *out_value = static_cast<T>(function(symbol));
        return PB_OK;
    });
}

template< typename Function >
PbStatus QuerySymbol(PbSymHandle symbol, PbSymHandle* out_symbol, Function function)
{
    if (!IsValid(symbol) || !out_symbol)
        return PB_ERR_INVALID_ARGUMENT;
    out_symbol->opaque = 0;
    return GuardSymbols([&]() -> PbStatus {
        out_symbol->opaque = function(symbol);
        return PB_OK;
    });
}

} // namespace

PbStatus PB_CALL pb_pin_undecorate_symbol_name(
    const char* symbol_name, PbUndecoration style,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!symbol_name || style > PB_UNDECORATION_NAME_ONLY ||
        !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = 0;
    return GuardSymbols([&]() {
        return PbBackendUndecorateSymbolName(
            symbol_name, style, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_sym_address(PbSymHandle symbol, uint64_t* out_address)
{
    return QueryScalar(symbol, out_address, PbBackendSymAddress);
}

PbStatus PB_CALL pb_sym_dynamic(PbSymHandle symbol, uint8_t* out_dynamic)
{
    return QueryScalar(symbol, out_dynamic, PbBackendSymDynamic);
}

PbStatus PB_CALL pb_sym_generated_by_pin(
    PbSymHandle symbol, uint8_t* out_generated)
{
    return QueryScalar(symbol, out_generated, PbBackendSymGeneratedByPin);
}

PbStatus PB_CALL pb_sym_index(PbSymHandle symbol, uint32_t* out_index)
{
    return QueryScalar(symbol, out_index, PbBackendSymIndex);
}

PbStatus PB_CALL pb_sym_invalid(PbSymHandle* out_symbol)
{
    if (!out_symbol)
        return PB_ERR_INVALID_ARGUMENT;
    out_symbol->opaque = 0;
    return GuardSymbols([&]() -> PbStatus {
        out_symbol->opaque = PbBackendSymInvalid();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_sym_name(
    PbSymHandle symbol, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    if (!IsValid(symbol) || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = 0;
    return GuardSymbols([&]() {
        return PbBackendSymName(symbol, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_sym_next(PbSymHandle symbol, PbSymHandle* out_symbol)
{
    return QuerySymbol(symbol, out_symbol, PbBackendSymNext);
}

PbStatus PB_CALL pb_sym_prev(PbSymHandle symbol, PbSymHandle* out_symbol)
{
    return QuerySymbol(symbol, out_symbol, PbBackendSymPrev);
}

PbStatus PB_CALL pb_sym_valid(PbSymHandle symbol, uint8_t* out_valid)
{
    if (!out_valid)
        return PB_ERR_INVALID_ARGUMENT;
    *out_valid = 0;
    return GuardSymbols([&]() -> PbStatus {
        *out_valid = PbBackendSymValid(symbol) ? 1u : 0u;
        return PB_OK;
    });
}

PbStatus PB_CALL pb_sym_value(PbSymHandle symbol, uint64_t* out_value)
{
    return QueryScalar(symbol, out_value, PbBackendSymValue);
}
