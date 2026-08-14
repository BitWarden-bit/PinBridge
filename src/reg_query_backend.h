#ifndef PINBRIDGE_REG_QUERY_BACKEND_H
#define PINBRIDGE_REG_QUERY_BACKEND_H

#include <stdint.h>

enum PbRegQueryId
{
#define PB_REG_QUERY0(return_kind, c_symbol, pin_symbol, api_id) PB_REG_QUERY_ID_##c_symbol,
#define PB_REG_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) PB_REG_QUERY_ID_##c_symbol,
#include "pinbridge/generated/reg_queries.inc"
#undef PB_REG_QUERY1
#undef PB_REG_QUERY0
    PB_REG_QUERY_ID_COUNT
};

uint64_t PbBackendRegQuery(uint32_t query_id, uint64_t argument);

#endif
