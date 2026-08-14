#ifndef PINBRIDGE_CONTEXT_CONSTANT_BACKEND_H
#define PINBRIDGE_CONTEXT_CONSTANT_BACKEND_H

#include <stdint.h>

enum PbContextConstantId
{
#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    PB_CONTEXT_CONSTANT_ID_##pin_symbol = index,
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT
    PB_CONTEXT_CONSTANT_ID_COUNT
};

uint64_t PbBackendContextConstant(uint32_t constant_id);

#endif
