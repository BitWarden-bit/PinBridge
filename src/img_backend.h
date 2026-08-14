#ifndef PINBRIDGE_IMG_BACKEND_H
#define PINBRIDGE_IMG_BACKEND_H

#include "pinbridge/pinbridge.h"

int32_t PbBackendAppImgHead(void);
int32_t PbBackendAppImgTail(void);
PbStatus PbBackendAddImgUnloadFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendImgClose(PbImgHandle image);
int32_t PbBackendImgFindByAddress(uint64_t address);
int32_t PbBackendImgFindById(uint32_t id);
int32_t PbBackendImgInvalid(void);
PbStatus PbBackendImgName(
    PbImgHandle image, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PbStatus PbBackendImgOpen(const char* filename, PbImgHandle* out_image);

#endif
