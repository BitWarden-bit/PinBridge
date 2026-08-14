#ifndef PINBRIDGE_SYSCALL_BACKEND_H
#define PINBRIDGE_SYSCALL_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendAddSyscallEntryFunction(
    PbSyscallEntryCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendAddSyscallExitFunction(
    PbSyscallExitCallback callback, void* user_data, uint64_t* out_callback);
PbStatus PbBackendGetSyscallArgument(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t* out_value);
PbStatus PbBackendGetSyscallErrno(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PbStatus PbBackendGetSyscallNumber(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PbStatus PbBackendGetSyscallReturn(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PbStatus PbBackendReplaySyscallEntry(
    PbThreadId thread_id, PbContextHandle context, PbSyscallStandard standard);
PbStatus PbBackendReplaySyscallExit(
    PbThreadId thread_id, PbContextHandle context, PbSyscallStandard standard);
PbStatus PbBackendSetSyscallArgument(
    PbContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t value);
PbStatus PbBackendSetSyscallErrno(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);
PbStatus PbBackendSetSyscallNumber(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);
PbStatus PbBackendSetSyscallReturn(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);

#endif
