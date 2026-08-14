#include "pin.H"

#include "control_fetch_backend.h"

namespace
{

struct FetchState
{
    PbFetchCallback callback;
    void* user_data;
};

FetchState g_fetch_state = {};
bool g_fetch_registered;

size_t OnFetch(
    void* buffer, ADDRINT address, size_t size,
    EXCEPTION_INFO* exception_info, VOID*)
{
    const uint64_t copied = g_fetch_state.callback(
        buffer, static_cast<uint64_t>(address), static_cast<uint64_t>(size),
        reinterpret_cast<PbExceptionInfoHandle>(exception_info),
        g_fetch_state.user_data);
    return copied <= static_cast<uint64_t>(size) ? static_cast<size_t>(copied) : size;
}

} // namespace

PbStatus PbBackendAddFetchFunction(PbFetchCallback callback, void* user_data)
{
    g_fetch_state.callback = callback;
    g_fetch_state.user_data = user_data;
    if (!g_fetch_registered)
    {
        PIN_AddFetchFunction(OnFetch, 0);
        g_fetch_registered = true;
    }
    return PB_OK;
}

uint64_t PbBackendFetchCode(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info)
{
    return static_cast<uint64_t>(PIN_FetchCode(
        copy_buffer, reinterpret_cast<const void*>(static_cast<uintptr_t>(address)),
        static_cast<size_t>(max_size),
        reinterpret_cast<EXCEPTION_INFO*>(exception_info)));
}
