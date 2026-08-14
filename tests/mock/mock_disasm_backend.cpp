#include "disasm_backend.h"

#include <cstring>

/* Mock disassembler: validates like the real facade path and emits one
   deterministic fake instruction per call. */

PbStatus PbBackendDisassemble(
    const uint8_t* bytes, uint64_t size, uint64_t address,
    PbDisasmInsn* out, uint64_t capacity, uint64_t* out_count)
{
    if (!bytes || size == 0 || !out || capacity == 0 || !out_count)
        return PB_ERR_INVALID_ARGUMENT;
    out[0].address = address;
    out[0].size = 1;
    out[0].kind = 0;
    std::memcpy(out[0].text, "nop", 4);
    *out_count = 1;
    return PB_OK;
}

PbStatus PbBackendDisassembleFlow(
    const uint8_t* bytes, uint64_t size, uint64_t address, PbFlowInsn* out)
{
    if (!bytes || size == 0 || !out)
        return PB_ERR_INVALID_ARGUMENT;
    out->address = address;
    out->size = 1;
    out->kind = 0;
    out->conditional = 0;
    out->has_target = 0;
    out->ind_reg = 0;
    out->ind_mem = 0;
    out->base_reg = -1;
    out->index_reg = -1;
    out->scale = 0;
    out->disp = 0;
    out->target = 0;
    return PB_OK;
}
