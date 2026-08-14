#include "syscall_backend.h"

namespace
{

uint64_t g_argument = UINT64_C(0xA0);
uint64_t g_number = UINT64_C(0xA1);
uint64_t g_return = UINT64_C(0xA2);
uint64_t g_errno = UINT64_C(0xA3);

} // namespace

PbStatus PbBackendAddSyscallEntryFunction(
    PbSyscallEntryCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x5601);
    callback(7, reinterpret_cast<PbContextHandle>(static_cast<uintptr_t>(0x5000)),
             PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST, user_data);
    return PB_OK;
}

PbStatus PbBackendAddSyscallExitFunction(
    PbSyscallExitCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x5602);
    callback(7, reinterpret_cast<PbContextHandle>(static_cast<uintptr_t>(0x5000)),
             PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST, user_data);
    return PB_OK;
}

PbStatus PbBackendGetSyscallArgument(
    PbConstContextHandle, PbSyscallStandard, uint32_t, uint64_t* out_value)
{
    *out_value = g_argument;
    return PB_OK;
}

PbStatus PbBackendGetSyscallErrno(
    PbConstContextHandle, PbSyscallStandard, uint64_t* out_value)
{
    *out_value = g_errno;
    return PB_OK;
}

PbStatus PbBackendGetSyscallNumber(
    PbConstContextHandle, PbSyscallStandard, uint64_t* out_value)
{
    *out_value = g_number;
    return PB_OK;
}

PbStatus PbBackendGetSyscallReturn(
    PbConstContextHandle, PbSyscallStandard, uint64_t* out_value)
{
    *out_value = g_return;
    return PB_OK;
}

PbStatus PbBackendReplaySyscallEntry(PbThreadId, PbContextHandle, PbSyscallStandard)
{ return PB_OK; }

PbStatus PbBackendReplaySyscallExit(PbThreadId, PbContextHandle, PbSyscallStandard)
{ return PB_OK; }

PbStatus PbBackendSetSyscallArgument(
    PbContextHandle, PbSyscallStandard, uint32_t, uint64_t value)
{ g_argument = value; return PB_OK; }

PbStatus PbBackendSetSyscallErrno(
    PbContextHandle, PbSyscallStandard, uint64_t value)
{ g_errno = value; return PB_OK; }

PbStatus PbBackendSetSyscallNumber(
    PbContextHandle, PbSyscallStandard, uint64_t value)
{ g_number = value; return PB_OK; }

PbStatus PbBackendSetSyscallReturn(
    PbContextHandle, PbSyscallStandard, uint64_t value)
{ g_return = value; return PB_OK; }
