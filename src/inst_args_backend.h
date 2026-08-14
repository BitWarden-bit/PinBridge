#ifndef PINBRIDGE_INST_ARGS_BACKEND_H
#define PINBRIDGE_INST_ARGS_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendIargListAlloc(PbIargListHandle* out_list);
PbStatus PbBackendIargListAdd(
    PbIargListHandle list, const PbIargDescriptor* descriptors,
    uint32_t descriptor_count);
PbStatus PbBackendIargListFree(PbIargListHandle list);
void* PbBackendIargListNative(PbIargListHandle list);

#endif
