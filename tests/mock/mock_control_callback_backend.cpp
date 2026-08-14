#include "control_callback_backend.h"

PbStatus PbBackendAddApplicationStartFunction(
    PbApplicationStartCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3001);
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendAddPrepareForFiniFunction(
    PbPrepareForFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3002);
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendAddFiniFunction(
    PbFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3003);
    callback(37, user_data);
    return PB_OK;
}

PbStatus PbBackendAddThreadStartFunction(
    PbThreadStartCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3004);
    callback(7u, reinterpret_cast<PbContextHandle>(static_cast<uintptr_t>(0x4000)),
             9, user_data);
    return PB_OK;
}

PbStatus PbBackendAddThreadFiniFunction(
    PbThreadFiniCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3005);
    callback(7u, reinterpret_cast<PbConstContextHandle>(static_cast<uintptr_t>(0x4000)),
             37, user_data);
    return PB_OK;
}

PbStatus PbBackendAddContextChangeFunction(
    PbContextChangeCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x3006);
    callback(7u, PB_CONTEXT_CHANGE_REASON_EXCEPTION,
        reinterpret_cast<PbConstContextHandle>(static_cast<uintptr_t>(0x4000)),
        reinterpret_cast<PbContextHandle>(static_cast<uintptr_t>(0x5000)),
        static_cast<int32_t>(UINT32_C(0xE0424242)), user_data);
    return PB_OK;
}

PbStatus PbBackendAddXedDecodeCallbackFunction(
    PbXedDecodeCallback callback, void* user_data)
{
    callback(reinterpret_cast<PbXedDecodedInstHandle>(static_cast<uintptr_t>(0x7000)),
             user_data);
    return PB_OK;
}
