#ifndef PINBRIDGE_PROTO_BACKEND_H
#define PINBRIDGE_PROTO_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendProtoAllocate(
    PbProtoArg return_arg, PbCallingStandard calling_standard,
    const char* name, const PbProtoArg* descriptors,
    uint32_t descriptor_count, PbProtoHandle* out_proto);
PbStatus PbBackendProtoFree(PbProtoHandle proto);

#endif
