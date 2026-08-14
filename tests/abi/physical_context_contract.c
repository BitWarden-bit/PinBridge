#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    uint8_t storage[PB_FXSAVE_SIZE + 32u] = {0};
    PbPhysicalContextHandle context =
        (PbPhysicalContextHandle)(void*)storage;
    PbConstPhysicalContextHandle const_context =
        (PbConstPhysicalContextHandle)(const void*)storage;
    PbFxSave input = {{0}};
    PbFxSave output = {{0}};
    uint64_t value = UINT64_C(0x1122334455667788);
    uint64_t read_value = 0;
    uint32_t index;

    if (pb_pin_get_physical_context_reg(
            const_context, PB_REG_GAX, &read_value) != PB_OK ||
        read_value != 0)
        return 1;
    if (pb_pin_set_physical_context_reg(context, PB_REG_GAX, value) != PB_OK ||
        pb_pin_get_physical_context_reg(
            const_context, PB_REG_GAX, &read_value) != PB_OK ||
        read_value != value)
        return 2;

    for (index = 0; index < PB_FXSAVE_SIZE; ++index)
        input.bytes[index] = (uint8_t)(index * 7u + 3u);
    if (pb_pin_set_physical_context_fxsave(context, &input) != PB_OK ||
        pb_pin_get_physical_context_fxsave(const_context, &output) != PB_OK ||
        memcmp(&input, &output, sizeof(input)) != 0)
        return 3;

    if (pb_pin_get_physical_context_reg(0, PB_REG_GAX, &read_value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_physical_context_reg(const_context, PB_REG_GAX, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_physical_context_reg(
            const_context, PB_REG_PHYSICAL_INTEGER_END + 1u, &read_value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_physical_context_reg(
            const_context, PB_REG_PHYSICAL_INTEGER_BASE - 1u, &read_value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_physical_context_reg(0, PB_REG_GAX, value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_physical_context_reg(
            context, PB_REG_PHYSICAL_INTEGER_END + 1u, value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_physical_context_fxsave(0, &output) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_physical_context_fxsave(const_context, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_physical_context_fxsave(0, &input) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_physical_context_fxsave(context, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 4;

    return 0;
}
