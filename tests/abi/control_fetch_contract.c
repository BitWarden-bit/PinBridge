#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_fetch_calls;

static uint64_t PB_CALL OnFetch(
    void* buffer, uint64_t address, uint64_t size,
    PbExceptionInfoHandle exception_info, void* user_data)
{
    uint8_t* bytes = (uint8_t*)buffer;
    uint64_t index;
    if (!buffer || address != UINT64_C(0x1000) || size != 4 ||
        !exception_info || user_data != &g_fetch_calls)
        return 0;
    for (index = 0; index < size; ++index)
        bytes[index] = (uint8_t)(index + 1u);
    ++g_fetch_calls;
    return size;
}

int main(void)
{
    uint8_t bytes[4] = {0};
    uint64_t copied = 99;

    if (pb_pin_add_fetch_function(OnFetch, &g_fetch_calls) != PB_OK || g_fetch_calls != 1)
        return 1;
    if (pb_pin_add_fetch_function(0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 2;
    if (pb_pin_fetch_code(bytes, UINT64_C(0x2000), sizeof(bytes), 0, &copied) != PB_OK ||
        copied != sizeof(bytes) || bytes[0] != 0x90 || bytes[2] != 0xcc)
        return 3;
    if (pb_pin_fetch_code(0, 0, 0, 0, &copied) != PB_OK || copied != 0)
        return 4;
    if (pb_pin_fetch_code(0, UINT64_C(0x2000), 1, 0, &copied) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_fetch_code(bytes, 0, sizeof(bytes), 0, &copied) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_fetch_code(bytes, UINT64_C(0x2000), sizeof(bytes), 0, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
