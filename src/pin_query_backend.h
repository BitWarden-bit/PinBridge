#ifndef PINBRIDGE_PIN_QUERY_BACKEND_H
#define PINBRIDGE_PIN_QUERY_BACKEND_H

#include <stdint.h>

enum PbInsQueryId
{
#define PB_INS_QUERY0(return_kind, c_symbol, pin_symbol, api_id) PB_INS_QUERY_ID_##c_symbol,
#define PB_INS_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) PB_INS_QUERY_ID_##c_symbol,
#include "pinbridge/generated/ins_inspection_queries.inc"
#undef PB_INS_QUERY1
#undef PB_INS_QUERY0
    PB_INS_QUERY_ID_COUNT
};

uint64_t PbBackendInsQuery(uint32_t query_id, int32_t ins, uint64_t argument);

#endif
