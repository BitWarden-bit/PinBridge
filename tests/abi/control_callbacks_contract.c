#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_application_calls;
static uint32_t g_prepare_calls;
static uint32_t g_fini_calls;
static uint32_t g_context_change_calls;
static uint32_t g_xed_decode_calls;

static void PB_CALL OnApplicationStart(void* user_data)
{
    if (user_data == &g_application_calls)
        ++g_application_calls;
}

static void PB_CALL OnPrepareForFini(void* user_data)
{
    if (user_data == &g_prepare_calls)
        ++g_prepare_calls;
}

static void PB_CALL OnFini(int32_t code, void* user_data)
{
    if (code == 37 && user_data == &g_fini_calls)
        ++g_fini_calls;
}

static void PB_CALL OnContextChange(
    PbThreadId thread_id, PbContextChangeReason reason,
    PbConstContextHandle from, PbContextHandle to, int32_t info, void* user_data)
{
    if (thread_id == 7u && reason == PB_CONTEXT_CHANGE_REASON_EXCEPTION &&
        from != 0 && to != 0 && (uint32_t)info == UINT32_C(0xE0424242) &&
        user_data == &g_context_change_calls)
        ++g_context_change_calls;
}

static void PB_CALL OnXedDecode(PbXedDecodedInstHandle decoded_instruction, void* user_data)
{
    if (decoded_instruction != 0 && user_data == &g_xed_decode_calls)
        ++g_xed_decode_calls;
}

int main(void)
{
    PbCallbackHandle callback = {99};

    if (pb_pin_add_application_start_function(
            OnApplicationStart, &g_application_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_application_calls != 1)
        return 1;
    if (pb_pin_add_prepare_for_fini_function(
            OnPrepareForFini, &g_prepare_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_prepare_calls != 1)
        return 2;
    if (pb_pin_add_fini_function(OnFini, &g_fini_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_fini_calls != 1)
        return 3;
    if (pb_pin_add_context_change_function(
            OnContextChange, &g_context_change_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_context_change_calls != 1)
        return 4;
    if (pb_pin_add_xed_decode_callback_function(OnXedDecode, &g_xed_decode_calls) != PB_OK ||
        g_xed_decode_calls != 1)
        return 5;
    if (pb_xed_decoded_inst_set_features(
            (PbXedDecodedInstHandle)(uintptr_t)UINT64_C(0x7000),
            PB_XED_DECODE_FEATURE_CET | PB_XED_DECODE_FEATURE_CLDEMOTE,
            PB_XED_DECODE_FEATURE_CLDEMOTE) != PB_OK)
        return 6;
    if (pb_pin_add_application_start_function(0, 0, &callback) !=
            PB_ERR_INVALID_ARGUMENT || callback.opaque != 0 ||
        pb_pin_add_prepare_for_fini_function(OnPrepareForFini, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_add_fini_function(0, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_add_context_change_function(0, 0, &callback) !=
            PB_ERR_INVALID_ARGUMENT || callback.opaque != 0 ||
        pb_pin_add_xed_decode_callback_function(0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_xed_decoded_inst_set_features(
            0, PB_XED_DECODE_FEATURE_CET, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_xed_decoded_inst_set_features(
            (PbXedDecodedInstHandle)(uintptr_t)UINT64_C(0x7000), 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_xed_decoded_inst_set_features(
            (PbXedDecodedInstHandle)(uintptr_t)UINT64_C(0x7000),
            PB_XED_DECODE_FEATURE_CET, PB_XED_DECODE_FEATURE_CLDEMOTE) !=
            PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
