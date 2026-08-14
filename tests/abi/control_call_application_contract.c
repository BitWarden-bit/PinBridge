#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbConstContextHandle context =
        (PbConstContextHandle)(uintptr_t)UINT64_C(0x1234);
    uint64_t result = 0;
    void* pointer_result = 0;

    if (pb_pin_call_application_function_void_0(
            context, 7, UINT64_C(0x1000)) != PB_OK)
        return 1;
    if (pb_pin_call_application_function_u64_0(
            context, 7, UINT64_C(0x2000), &result) != PB_OK ||
        result != UINT64_C(0x2000))
        return 2;
    if (pb_pin_call_application_function_u64_1(
            context, 7, UINT64_C(0x3000), 11, &result) != PB_OK ||
        result != UINT64_C(0x300b))
        return 3;
    if (pb_pin_call_application_function_u64_2(
            context, 7, UINT64_C(0x4000), 11, 13, &result) != PB_OK ||
        result != UINT64_C(0x4018))
        return 4;
    if (pb_pin_call_application_function_ptr_usize(
            context, 7, UINT64_C(0x5000), 32, &pointer_result) != PB_OK ||
        pointer_result != (void*)(uintptr_t)UINT64_C(0x5020))
        return 5;

    result = 99;
    pointer_result = (void*)(uintptr_t)1;
    if (pb_pin_call_application_function_u64_0(
            0, 7, UINT64_C(0x2000), &result) != PB_ERR_INVALID_ARGUMENT ||
        result != 0)
        return 6;
    if (pb_pin_call_application_function_u64_1(
            context, 7, 0, 11, &result) != PB_ERR_INVALID_ARGUMENT ||
        result != 0)
        return 7;
    if (pb_pin_call_application_function_u64_2(
            context, 7, UINT64_C(0x4000), 11, 13, 0) !=
        PB_ERR_INVALID_ARGUMENT)
        return 8;
    if (pb_pin_call_application_function_ptr_usize(
            context, 7, 0, 32, &pointer_result) != PB_ERR_INVALID_ARGUMENT ||
        pointer_result != 0)
        return 9;
    if (pb_pin_call_application_function_void_0(context, 7, 0) !=
        PB_ERR_INVALID_ARGUMENT)
        return 10;
    return 0;
}
