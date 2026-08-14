#ifndef PINBRIDGE_PROCESS_FOLLOW_CHILD_BACKEND_H
#define PINBRIDGE_PROCESS_FOLLOW_CHILD_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendChildProcessGetCommandLineCount(
    PbChildProcessHandle child, int32_t* out_argc);
PbStatus PbBackendChildProcessGetCommandLineArgument(
    PbChildProcessHandle child, int32_t index, char* buffer,
    uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendChildProcessGetId(
    PbChildProcessHandle child, uint32_t* out_process_id);
PbStatus PbBackendChildProcessSetPinCommandLine(
    PbChildProcessHandle child, int32_t argc, const char* const* argv);
PbStatus PbBackendAddFollowChildProcessFunction(
    PbFollowChildProcessCallback callback, void* user_data, uint64_t* out_callback);

#endif
