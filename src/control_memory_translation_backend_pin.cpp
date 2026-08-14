#include "pin.H"

#include "control_memory_translation_backend.h"

namespace
{

struct MemoryTranslationState
{
    PbMemoryAddressTransCallback callback;
    void* user_data;
};

MemoryTranslationState g_memory_translation_state = {};
bool g_memory_translation_registered;
MEMORY_ADDR_TRANS_CALLBACK g_registered_pin_callback;

ADDRINT PIN_FAST_ANALYSIS_CALL OnMemoryAddressTrans(
    PIN_MEM_TRANS_INFO* pin_info, VOID*)
{
    PbMemoryTransInfo info = {};
    info.address = static_cast<uint64_t>(pin_info->addr);
    info.size = static_cast<uint64_t>(pin_info->bytes);
    info.instruction_pointer = static_cast<uint64_t>(pin_info->ip);
    info.thread_id = static_cast<PbThreadId>(pin_info->threadIndex);
    info.memory_operation = static_cast<uint32_t>(pin_info->memOpType);
    info.is_atomic = pin_info->flags.bits.isAtomic ? 1u : 0u;
    info.is_rmw = pin_info->flags.bits.isRmw ? 1u : 0u;
    info.is_prefetch = pin_info->flags.bits.isPrefetch ? 1u : 0u;
    info.is_from_pin = pin_info->flags.bits.isFromPin ? 1u : 0u;
    return static_cast<ADDRINT>(g_memory_translation_state.callback(
        &info, g_memory_translation_state.user_data));
}

} // namespace

PbStatus PbBackendAddMemoryAddressTransFunction(
    PbMemoryAddressTransCallback callback, void* user_data)
{
    g_memory_translation_state.callback = callback;
    g_memory_translation_state.user_data = user_data;
    if (!g_memory_translation_registered)
    {
        PIN_AddMemoryAddressTransFunction(OnMemoryAddressTrans, 0);
        g_registered_pin_callback = PIN_GetMemoryAddressTransFunction();
        if (!g_registered_pin_callback)
            return PB_ERR_INTERNAL;
        g_memory_translation_registered = true;
    }
    return PB_OK;
}

PbStatus PbBackendGetMemoryAddressTransFunction(
    PbMemoryAddressTransCallback* out_callback)
{
    const MEMORY_ADDR_TRANS_CALLBACK pin_callback =
        PIN_GetMemoryAddressTransFunction();
    if (!pin_callback)
        return PB_OK;
    if (!g_memory_translation_registered ||
        pin_callback != g_registered_pin_callback)
        return PB_ERR_INVALID_STATE;
    *out_callback = g_memory_translation_state.callback;
    return PB_OK;
}
