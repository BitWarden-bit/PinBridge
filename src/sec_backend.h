#ifndef PINBRIDGE_SEC_BACKEND_H
#define PINBRIDGE_SEC_BACKEND_H

#include "pinbridge/pinbridge.h"

uint64_t PbBackendSecData(uint32_t sec);
int32_t PbBackendSecInvalid(void);
PbStatus PbBackendSecName(
    uint32_t sec, char* buffer, uint64_t capacity, uint64_t* required_size);

#endif
