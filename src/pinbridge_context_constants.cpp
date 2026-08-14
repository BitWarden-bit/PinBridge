#include "pinbridge/pinbridge.h"

#include "context_constant_backend.h"

namespace
{

PbStatus Query(uint32_t constant_id, uint64_t* out_value)
{
    if (!out_value)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
#endif
        *out_value = PbBackendContextConstant(constant_id);
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

#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol(uint64_t* out_value) \
    { \
        return Query(PB_CONTEXT_CONSTANT_ID_##pin_symbol, out_value); \
    }
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT
