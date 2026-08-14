#include "structure_callback_backend.h"

PbStatus PbBackendAddTraceInstrumentFunction(
    PbTraceInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x2001);
    callback(reinterpret_cast<PbTraceHandle>(static_cast<uintptr_t>(0x1000)), user_data);
    return PB_OK;
}

PbStatus PbBackendAddRtnInstrumentFunction(
    PbRtnInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x2002);
    const PbRtnHandle handle = {43};
    callback(handle, user_data);
    return PB_OK;
}

PbStatus PbBackendAddImgInstrumentFunction(
    PbImgInstrumentCallback callback, void* user_data, uint64_t* out_callback)
{
    *out_callback = UINT64_C(0x2003);
    const PbImgHandle handle = {44};
    callback(handle, user_data);
    return PB_OK;
}
