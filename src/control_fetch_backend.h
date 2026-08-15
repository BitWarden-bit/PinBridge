#ifndef PINBRIDGE_CONTROL_FETCH_BACKEND_H
#define PINBRIDGE_CONTROL_FETCH_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddFetchFunction(PbFetchCallback callback, void* user_data);
uint64_t PbBackendFetchCode(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info);
uint64_t PbBackendFetchOriginalCode(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info);

#endif
