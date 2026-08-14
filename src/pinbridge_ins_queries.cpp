#include "pinbridge/pinbridge.h"

#include "pin_query_backend.h"

namespace
{

template< typename Function > PbStatus GuardQuery(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

template< typename T > void StoreResult(uint64_t bits, T* output)
{
    *output = static_cast<T>(bits);
}

void StoreResult(uint64_t bits, PbInsHandle* output)
{
    output->opaque = static_cast<int32_t>(bits);
}

void StoreResult(uint64_t bits, PbRtnHandle* output)
{
    output->opaque = static_cast<int32_t>(bits);
}

template< typename T > PbStatus Query0(PbInsHandle ins, uint32_t query_id, T* output)
{
    if (ins.opaque <= 0 || !output)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        StoreResult(PbBackendInsQuery(query_id, ins.opaque, 0), output);
        return PB_OK;
    });
}

template< typename T > PbStatus Query1(PbInsHandle ins, uint32_t query_id, uint64_t argument, T* output)
{
    if (ins.opaque <= 0 || !output)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        StoreResult(PbBackendInsQuery(query_id, ins.opaque, argument), output);
        return PB_OK;
    });
}

} // namespace

#define PB_INS_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol(PbInsHandle ins, PB_INS_C_TYPE_##return_kind* out_value) \
    { \
        return Query0(ins, PB_INS_QUERY_ID_##c_symbol, out_value); \
    }
#define PB_INS_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol( \
        PbInsHandle ins, PB_INS_C_ARG_##argument_kind argument, \
        PB_INS_C_TYPE_##return_kind* out_value) \
    { \
        return Query1(ins, PB_INS_QUERY_ID_##c_symbol, static_cast<uint64_t>(argument), out_value); \
    }
#include "pinbridge/generated/ins_inspection_queries.inc"
#undef PB_INS_QUERY1
#undef PB_INS_QUERY0
