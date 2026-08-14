#include "pin.H"

#include "pin_query_backend.h"

namespace
{

static_assert(sizeof(INS) == sizeof(int32_t), "Pin 3.31 INS no longer fits PbInsHandle");
static_assert(sizeof(RTN) == sizeof(int32_t), "Pin 3.31 RTN no longer fits PbRtnHandle");

INS ToQueryIns(int32_t value)
{
    INS ins;
    ins.q_set(value);
    return ins;
}

template< typename T > uint64_t ToQueryBits(T value)
{
    return static_cast<uint64_t>(value);
}

uint64_t ToQueryBits(INS value) { return static_cast<uint64_t>(value.q()); }
uint64_t ToQueryBits(RTN value) { return static_cast<uint64_t>(value.q()); }

} // namespace

uint64_t PbBackendInsQuery(uint32_t query_id, int32_t ins_value, uint64_t argument)
{
    const INS ins = ToQueryIns(ins_value);
    switch (query_id)
    {
#define PB_PIN_ARG_UINT32(value) static_cast<UINT32>(value)
#define PB_PIN_ARG_REG(value) static_cast<REG>(value)
#define PB_PIN_ARG_IARG_TYPE(value) static_cast<IARG_TYPE>(value)
#define PB_INS_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    case PB_INS_QUERY_ID_##c_symbol: return ToQueryBits(pin_symbol(ins));
#define PB_INS_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    case PB_INS_QUERY_ID_##c_symbol: \
        return ToQueryBits(pin_symbol(ins, PB_PIN_ARG_##argument_kind(argument)));
#include "pinbridge/generated/ins_inspection_queries.inc"
#undef PB_INS_QUERY1
#undef PB_INS_QUERY0
#undef PB_PIN_ARG_IARG_TYPE
#undef PB_PIN_ARG_REG
#undef PB_PIN_ARG_UINT32
    default: return 0;
    }
}
