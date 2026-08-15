#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbRegSet full_context_regs = {{0}};
    uint8_t source_storage[2048] = {0};
    uint8_t copy_storage[2048] = {0};
    uint8_t vector_value[16];
    uint8_t output[16] = {0};
    uint64_t required = 0;
    uint64_t scalar = UINT64_C(0x1122334455667788);
    uint64_t stack_arg = 0;
    uint8_t supported = 0;
    uint64_t context_constant = 0;
    uint8_t fpstate[64];
    uint8_t fpstate_output[64] = {0};
    PbFxSave fxsave = {{0}};
    PbFxSave fxsave_output = {{0}};
    uint32_t index;
    PbContextHandle source = (PbContextHandle)(void*)source_storage;
    PbContextHandle copy = (PbContextHandle)(void*)copy_storage;
    PbConstContextHandle const_source = (PbConstContextHandle)(const void*)source_storage;
    PbConstContextHandle const_copy = (PbConstContextHandle)(const void*)copy_storage;

    if (pb_pin_execute_at(0) != PB_ERR_INVALID_ARGUMENT)
        return 39;

    if (pb_pin_get_full_context_regs_set(&full_context_regs) != PB_OK ||
        full_context_regs.words[0] != UINT64_C(0x6))
        return 7;
    for (index = 1; index < PB_REGSET_WORD_COUNT; ++index)
        if (full_context_regs.words[index] != 0)
            return 7;
    if (pb_pin_get_full_context_regs_set(0) != PB_ERR_INVALID_ARGUMENT)
        return 8;

    if (pb_pin_supports_processor_state(PB_PROCESSOR_STATE_X87, &supported) != PB_OK ||
        supported != 1u)
        return 9;
    if (pb_pin_supports_processor_state(PB_PROCESSOR_STATE_ZMM, &supported) != PB_OK ||
        supported != 0u)
        return 10;
    if (pb_pin_context_contains_state(copy, PB_PROCESSOR_STATE_XMM, &supported) != PB_OK ||
        supported != 1u)
        return 11;
    if (pb_pin_context_contains_state(copy, PB_PROCESSOR_STATE_TMM, &supported) != PB_OK ||
        supported != 0u)
        return 12;
    if (pb_pin_supports_processor_state((PbProcessorState)UINT32_MAX, &supported) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_supports_processor_state(PB_PROCESSOR_STATE_X87, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_context_contains_state(0, PB_PROCESSOR_STATE_X87, &supported) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_context_contains_state(copy, (PbProcessorState)UINT32_MAX, &supported) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_context_contains_state(copy, PB_PROCESSOR_STATE_X87, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 13;

#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    if (c_symbol(&context_constant) != PB_OK || context_constant != (uint64_t)((index) + 1u)) \
        return 20 + (index);
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT
    if (pb_context_arch_state_size(0) != PB_ERR_INVALID_ARGUMENT)
        return 35;

    for (index = 0; index < sizeof(fpstate); ++index)
        fpstate[index] = (uint8_t)(index * 3u + 1u);
    if (pb_pin_get_context_fpstate(const_copy, 0, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        required != sizeof(fpstate) ||
        pb_pin_set_context_fpstate(copy, fpstate, sizeof(fpstate)) != PB_OK ||
        pb_pin_get_context_fpstate(
            const_copy, fpstate_output, sizeof(fpstate_output), &required) != PB_OK ||
        memcmp(fpstate, fpstate_output, sizeof(fpstate)) != 0)
        return 36;

    for (index = 0; index < PB_FXSAVE_SIZE; ++index)
        fxsave.bytes[index] = (uint8_t)(index * 5u + 7u);
    if (pb_pin_set_context_fxsave(copy, &fxsave) != PB_OK ||
        pb_pin_get_context_fxsave(const_copy, &fxsave_output) != PB_OK ||
        memcmp(&fxsave, &fxsave_output, sizeof(fxsave)) != 0)
        return 37;

    if (pb_pin_get_context_fpstate(0, fpstate_output, sizeof(fpstate_output), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_fpstate(const_copy, 0, sizeof(fpstate_output), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_fpstate(const_copy, fpstate_output, sizeof(fpstate_output), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_fpstate(0, fpstate, sizeof(fpstate)) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_fpstate(copy, 0, sizeof(fpstate)) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_fpstate(copy, fpstate, sizeof(fpstate) - 1u) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_fxsave(0, &fxsave_output) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_fxsave(const_copy, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_fxsave(0, &fxsave) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_fxsave(copy, 0) != PB_ERR_INVALID_ARGUMENT)
        return 38;

    for (index = 0; index < sizeof(vector_value); ++index)
        vector_value[index] = (uint8_t)(index + 1u);

    if (pb_pin_set_context_reg(source, 1, scalar) != PB_OK ||
        pb_pin_save_context(const_source, copy) != PB_OK)
        return 1;
    if (pb_pin_get_context_regval(const_copy, 1, 0, 0, &required) !=
            PB_ERR_BUFFER_TOO_SMALL ||
        required != sizeof(scalar))
        return 2;
    if (pb_pin_get_context_regval(const_copy, 1, output, sizeof(output), &required) != PB_OK ||
        memcmp(output, &scalar, sizeof(scalar)) != 0)
        return 3;

    if (pb_pin_set_context_regval(copy, 2, vector_value, sizeof(vector_value)) != PB_OK ||
        pb_pin_get_context_regval(const_copy, 2, output, sizeof(output), &required) != PB_OK ||
        required != sizeof(vector_value) || memcmp(output, vector_value, sizeof(output)) != 0)
        return 4;
    if (pb_pin_set_context_regval(copy, 2, vector_value, sizeof(vector_value) - 1) !=
        PB_ERR_INVALID_ARGUMENT)
        return 5;

    if (pb_pin_save_context(0, copy) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_save_context(const_source, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_reg(0, 1, scalar) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_regval(0, 2, vector_value, sizeof(vector_value)) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_regval(copy, 2, 0, sizeof(vector_value)) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_regval(0, 2, output, sizeof(output), &required) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_regval(const_copy, 2, output, sizeof(output), 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 6;

    if (pb_pin_set_context_stack_arg(copy, 3, UINT64_C(0xaabbccddeeff0011)) != PB_OK ||
        pb_pin_get_context_stack_arg(const_copy, 3, &stack_arg) != PB_OK ||
        stack_arg != UINT64_C(0xaabbccddeeff0011))
        return 40;
    if (pb_pin_get_context_stack_arg(0, 3, &stack_arg) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_stack_arg(const_copy, 3, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_stack_arg(0, 3, 1) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_context_stack_arg(const_copy, 32, &stack_arg) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_set_context_stack_arg(copy, 32, 1) != PB_ERR_INVALID_ARGUMENT)
        return 41;

    return 0;
}
