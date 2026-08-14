#include "pinbridge/pinbridge.h"

#include "reg_query_backend.h"

namespace
{

template< typename T > PbStatus Query(uint32_t query_id, uint64_t argument, T* output)
{
    if (!output)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        *output = static_cast<T>(PbBackendRegQuery(query_id, argument));
        return PB_OK;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#endif
}

} // namespace

#define PB_REG_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol(PB_REG_C_TYPE_##return_kind* out_value) \
    { \
        return Query(PB_REG_QUERY_ID_##c_symbol, 0, out_value); \
    }
#define PB_REG_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol( \
        PB_REG_C_ARG_##argument_kind argument, PB_REG_C_TYPE_##return_kind* out_value) \
    { \
        return Query(PB_REG_QUERY_ID_##c_symbol, static_cast<uint64_t>(argument), out_value); \
    }
#include "pinbridge/generated/reg_queries.inc"
#undef PB_REG_QUERY1
#undef PB_REG_QUERY0
