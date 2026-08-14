#ifndef PINBRIDGE_BUFFER_BACKEND_H
#define PINBRIDGE_BUFFER_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendDefineTraceBuffer(
    uint64_t record_size, uint32_t num_pages,
    PbTraceBufferCallback callback, void* user_data, PbBufferId* out_id);
PbStatus PbBackendAllocateBuffer(PbBufferId id, void** out_buffer);
PbStatus PbBackendDeallocateBuffer(PbBufferId id, void* buffer);
PbStatus PbBackendGetBufferPointer(
    PbContextHandle context, PbBufferId id, void** out_buffer);

#endif
