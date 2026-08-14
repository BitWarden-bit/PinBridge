#ifndef PINBRIDGE_PIN_BACKEND_H
#define PINBRIDGE_PIN_BACKEND_H

#include <stdint.h>

#include "pinbridge/pinbridge.h"

struct PbBackend
{
    const char* (*version)(void);
    int32_t (*init)(int32_t argc, char** argv);
    void (*start_program_default)(void);
    PbStatus (*add_ins_instrument_function)(PbInsInstrumentCallback callback, void* user_data, uint64_t* out_callback);
    uint64_t (*ins_address)(int32_t ins);
    uint64_t (*ins_size)(int32_t ins);
    uint64_t (*get_context_reg)(const void* context, uint32_t reg);
    uint64_t (*safe_copy)(void* destination, uint64_t source_address, uint64_t size);
    uint64_t (*safe_copy_ex)(void* destination, uint64_t source_address, uint64_t size,
        PbExceptionInfoSnapshot* out_exception);
};

const PbBackend& PbGetBackend(void);

#endif
