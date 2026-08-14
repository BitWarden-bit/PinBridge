#ifndef PINBRIDGE_SYMBOLS_BACKEND_H
#define PINBRIDGE_SYMBOLS_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendUndecorateSymbolName(
    const char* symbol_name, PbUndecoration style,
    char* buffer, uint64_t capacity, uint64_t* required_size);
uint64_t PbBackendSymAddress(PbSymHandle symbol);
uint8_t PbBackendSymDynamic(PbSymHandle symbol);
uint8_t PbBackendSymGeneratedByPin(PbSymHandle symbol);
uint32_t PbBackendSymIndex(PbSymHandle symbol);
int32_t PbBackendSymInvalid(void);
PbStatus PbBackendSymName(
    PbSymHandle symbol, char* buffer, uint64_t capacity,
    uint64_t* required_size);
int32_t PbBackendSymNext(PbSymHandle symbol);
int32_t PbBackendSymPrev(PbSymHandle symbol);
uint8_t PbBackendSymValid(PbSymHandle symbol);
uint64_t PbBackendSymValue(PbSymHandle symbol);

#endif
