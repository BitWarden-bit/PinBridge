#include "control_call_application_backend.h"

PbStatus PbBackendCallApplicationVoid0(
    PbConstContextHandle, PbThreadId, uint64_t)
{
    return PB_OK;
}

PbStatus PbBackendCallApplicationU640(
    PbConstContextHandle, PbThreadId, uint64_t function_address,
    uint64_t* out_result)
{
    *out_result = function_address;
    return PB_OK;
}

PbStatus PbBackendCallApplicationU641(
    PbConstContextHandle, PbThreadId, uint64_t function_address,
    uint64_t argument0, uint64_t* out_result)
{
    *out_result = function_address + argument0;
    return PB_OK;
}

PbStatus PbBackendCallApplicationU642(
    PbConstContextHandle, PbThreadId, uint64_t function_address,
    uint64_t argument0, uint64_t argument1, uint64_t* out_result)
{
    *out_result = function_address + argument0 + argument1;
    return PB_OK;
}

PbStatus PbBackendCallApplicationPtrUsize(
    PbConstContextHandle, PbThreadId, uint64_t function_address,
    uint64_t size, void** out_result)
{
    *out_result = reinterpret_cast<void*>(
        static_cast<uintptr_t>(function_address + size));
    return PB_OK;
}
