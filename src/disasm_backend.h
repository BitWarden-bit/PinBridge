#ifndef PINBRIDGE_DISASM_BACKEND_H
#define PINBRIDGE_DISASM_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendDisassemble(
    const uint8_t* bytes, uint64_t size, uint64_t address,
    PbDisasmInsn* out, uint64_t capacity, uint64_t* out_count);
PbStatus PbBackendDisassembleFlow(
    const uint8_t* bytes, uint64_t size, uint64_t address, PbFlowInsn* out);

#endif
