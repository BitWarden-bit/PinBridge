#ifndef PINBRIDGE_CONTEXT_BACKEND_H
#define PINBRIDGE_CONTEXT_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendGetContextRegval(
    const void* context, PbRegId reg, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendGetFullContextRegsSet(PbRegSet* out_regs);
PbStatus PbBackendGetContextFpState(
    const void* context, uint8_t* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendSetContextFpState(
    void* context, const uint8_t* value, uint64_t value_size);
PbStatus PbBackendGetContextFxSave(const void* context, PbFxSave* out_fxsave);
PbStatus PbBackendSetContextFxSave(void* context, const PbFxSave* fxsave);
PbStatus PbBackendSupportsProcessorState(PbProcessorState state, uint8_t* out_supported);
PbStatus PbBackendContextContainsState(
    void* context, PbProcessorState state, uint8_t* out_contains);
PbStatus PbBackendSaveContext(const void* source, void* destination);
PbStatus PbBackendSetContextReg(void* context, PbRegId reg, uint64_t value);
PbStatus PbBackendSetContextRegval(
    void* context, PbRegId reg, const uint8_t* value, uint64_t value_size);
PB_NORETURN void PbBackendExecuteAt(const void* context);

#endif
