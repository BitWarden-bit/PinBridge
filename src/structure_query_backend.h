#ifndef PINBRIDGE_STRUCTURE_QUERY_BACKEND_H
#define PINBRIDGE_STRUCTURE_QUERY_BACKEND_H

#include "pinbridge/pinbridge.h"

enum PbStructureQueryId
{
#define PB_HANDLE_QUERY0(input_kind, return_kind, c_symbol, pin_symbol, api_id) \
    PB_STRUCTURE_QUERY_ID_##c_symbol,
#define PB_HANDLE_QUERY1(input_kind, return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PB_STRUCTURE_QUERY_ID_##c_symbol,
#include "pinbridge/generated/structure_queries.inc"
#undef PB_HANDLE_QUERY1
#undef PB_HANDLE_QUERY0
    PB_STRUCTURE_QUERY_ID_COUNT
};

uint64_t PbBackendStructureQuery(uint32_t query_id, uint64_t input, uint64_t argument);
PbStatus PbBackendRtnClose(int32_t routine);
PbStatus PbBackendRtnCreateAt(
    uint64_t address, const char* name, int32_t* out_routine);
int32_t PbBackendRtnFindByAddress(uint64_t address);
int32_t PbBackendRtnFindByName(int32_t image, const char* name);
PbStatus PbBackendRtnFindNameByAddress(
    uint64_t address, char* buffer, uint64_t capacity,
    uint64_t* required_size);
uint64_t PbBackendRtnFunptr(int32_t routine);
int32_t PbBackendRtnInvalid(void);
PbStatus PbBackendRtnName(
    int32_t routine, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PbStatus PbBackendRtnOpen(int32_t routine);
PbStatus PbBackendRtnReplace(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address);
PbStatus PbBackendRtnReplaceProbed(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address);
PbStatus PbBackendRtnReplaceProbedEx(
    int32_t routine, PbProbeMode mode, uint64_t replacement_address,
    uint64_t* out_original_address);

#endif
