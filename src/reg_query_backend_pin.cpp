#include "pin.H"

#include "reg_query_backend.h"
#include "reg_mapping_pin.h"

namespace
{

template< typename T > uint64_t ToBits(T value) { return static_cast<uint64_t>(value); }
uint64_t ToBits(REG value)
{
    PbRegId public_reg = PB_REG_NONE;
    return PbRegIdFromPinReg(value, &public_reg) ?
        static_cast<uint64_t>(public_reg) : static_cast<uint64_t>(PB_REG_NONE);
}
REG PbPinRegArg(PbRegId value)
{
    REG native = REG_INVALID();
    PbPinRegFromId(value, &native);
    return native;
}

#define PB_PIN_REG_ARG_REG(value) PbPinRegArg(static_cast<PbRegId>(value))
#define PB_PIN_REG_ARG_UINT16(value) static_cast<UINT16>(value)

} // namespace

uint64_t PbBackendRegQuery(uint32_t query_id, uint64_t argument)
{
    switch (query_id)
    {
#define PB_REG_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    case PB_REG_QUERY_ID_##c_symbol: return ToBits(pin_symbol());
#define PB_REG_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    case PB_REG_QUERY_ID_##c_symbol: \
        return ToBits(pin_symbol(PB_PIN_REG_ARG_##argument_kind(argument)));
#include "pinbridge/generated/reg_queries.inc"
#undef PB_REG_QUERY1
#undef PB_REG_QUERY0
    default: return 0;
    }
}
