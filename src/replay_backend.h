#ifndef PINBRIDGE_REPLAY_BACKEND_H
#define PINBRIDGE_REPLAY_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendImgCreateAt(
    const char* filename, uint64_t start, uint64_t size, uint64_t load_offset,
    uint8_t main_executable, PbImgHandle* out_image);
PbStatus PbBackendImgReplayImageLoad(PbImgHandle image);
PbStatus PbBackendReplayContextChange(
    PbThreadId thread_id, PbConstContextHandle from, PbContextHandle to,
    PbContextChangeReason reason, int32_t info);

#endif
