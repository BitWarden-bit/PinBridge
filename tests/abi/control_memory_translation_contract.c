#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_calls;

static uint64_t PB_CALL OnMemoryAddressTrans(
    const PbMemoryTransInfo* info, void* user_data)
{
    if (!info || info->thread_id != 7u || info->address != UINT64_C(0x1000) ||
        info->size != 8u || info->instruction_pointer != UINT64_C(0x2000) ||
        info->memory_operation != 2u || info->is_atomic != 1u ||
        info->is_rmw != 0u || info->is_prefetch != 0u ||
        info->is_from_pin != 0u || info->reserved != 0u ||
        user_data != &g_calls)
        return 0;
    ++g_calls;
    return info->address + UINT64_C(0x10);
}

int main(void)
{
    PbMemoryAddressTransCallback callback = OnMemoryAddressTrans;

    if (pb_pin_get_memory_address_trans_function(&callback) != PB_OK || callback != 0)
        return 1;
    if (pb_pin_add_memory_address_trans_function(OnMemoryAddressTrans, &g_calls) != PB_OK ||
        g_calls != 1u)
        return 2;
    if (pb_pin_get_memory_address_trans_function(&callback) != PB_OK ||
        callback != OnMemoryAddressTrans)
        return 3;
    if (pb_pin_add_memory_address_trans_function(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_memory_address_trans_function(0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
