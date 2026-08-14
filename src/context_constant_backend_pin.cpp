#include "pin.H"

#include "context_constant_backend.h"

uint64_t PbBackendContextConstant(uint32_t constant_id)
{
    switch (constant_id)
    {
#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    case PB_CONTEXT_CONSTANT_ID_##pin_symbol: return static_cast<uint64_t>(pin_symbol);
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT
    default: return 0;
    }
}
