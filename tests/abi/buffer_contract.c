#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_callback_calls;

static void* PB_CALL OnBuffer(
    PbBufferId id, PbThreadId thread_id, PbConstContextHandle context,
    void* buffer, uint64_t num_elements, void* user_data)
{
    if (id == (PbBufferId)7 && thread_id == (PbThreadId)9 && context &&
        buffer && num_elements == UINT64_C(11) && user_data == &g_callback_calls)
        ++g_callback_calls;
    return buffer;
}

int main(void)
{
    PbBufferId id = PB_BUFFER_ID_INVALID;
    void* buffer = 0;
    PbContextHandle context = (PbContextHandle)(uintptr_t)1;
    if (sizeof(PbBufferId) != 4 || PB_BUFFER_ID_INVALID != 0)
        return 1;
    if (pb_pin_define_trace_buffer(
            16, 2, OnBuffer, &g_callback_calls, &id) != PB_OK || id != 7)
        return 2;
    if (pb_pin_allocate_buffer(id, &buffer) != PB_OK || !buffer)
        return 3;
    if (pb_pin_get_buffer_pointer(context, id, &buffer) != PB_OK || !buffer)
        return 4;
    if (pb_pin_deallocate_buffer(id, buffer) != PB_OK || g_callback_calls != 1)
        return 5;
    if (pb_pin_define_trace_buffer(0, 1, OnBuffer, 0, &id) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_define_trace_buffer(16, 0, OnBuffer, 0, &id) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_define_trace_buffer(16, 1, 0, 0, &id) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_define_trace_buffer(16, 1, OnBuffer, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_allocate_buffer(PB_BUFFER_ID_INVALID, &buffer) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_allocate_buffer(id, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_deallocate_buffer(id, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_buffer_pointer(0, id, &buffer) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_get_buffer_pointer(context, id, 0) != PB_ERR_INVALID_ARGUMENT)
        return 6;
    return 0;
}
