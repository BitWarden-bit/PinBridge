#ifndef PINBRIDGE_PHYSICAL_CONTEXT_BACKEND_H
#define PINBRIDGE_PHYSICAL_CONTEXT_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendGetPhysicalContextReg(
    const void* context, PbRegId reg, uint64_t* out_value);
PbStatus PbBackendSetPhysicalContextReg(
    void* context, PbRegId reg, uint64_t value);
PbStatus PbBackendGetPhysicalContextFxSave(
    const void* context, PbFxSave* out_fxsave);
PbStatus PbBackendSetPhysicalContextFxSave(
    void* context, const PbFxSave* fxsave);

#endif
