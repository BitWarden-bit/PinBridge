#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_callback_calls;

static uint8_t PB_CALL OnFollowChild(PbChildProcessHandle child, void* user_data)
{
    int32_t argc = 0;
    uint32_t process_id = 0;
    uint64_t required = 0;
    char argument[32];
    const char* pin_argv[] = {"pin.exe", "-t", "tool.dll", "--"};

    if (user_data != &g_callback_calls || child == 0)
        return 0;
    if (pb_child_process_get_id(child, &process_id) != PB_OK ||
        process_id != UINT32_C(0x12345678))
        return 0;
    if (pb_child_process_get_command_line_count(child, &argc) != PB_OK || argc != 3)
        return 0;
    if (pb_child_process_get_command_line_argument(
            child, 1, 0, 0, &required) != PB_ERR_BUFFER_TOO_SMALL || required != 10)
        return 0;
    memset(argument, 0x5a, sizeof(argument));
    if (pb_child_process_get_command_line_argument(
            child, 1, argument, sizeof(argument), &required) != PB_OK ||
        strcmp(argument, "--payload") != 0)
        return 0;
    if (pb_child_process_set_pin_command_line(child, 4, pin_argv) != PB_OK)
        return 0;
    ++g_callback_calls;
    return 1;
}

int main(void)
{
    PbCallbackHandle callback = {99};
    uint64_t required = 99;

    if (pb_pin_add_follow_child_process_function(
            OnFollowChild, &g_callback_calls, &callback) != PB_OK ||
        callback.opaque == 0 || g_callback_calls != 1)
        return 1;
    if (pb_pin_add_follow_child_process_function(0, 0, &callback) !=
            PB_ERR_INVALID_ARGUMENT || callback.opaque != 0)
        return 2;
    if (pb_pin_add_follow_child_process_function(
            OnFollowChild, &g_callback_calls, &callback) != PB_ERR_INVALID_STATE ||
        callback.opaque != 0)
        return 3;
    if (pb_child_process_get_command_line_argument(
            0, 0, 0, 0, &required) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    if (pb_child_process_set_pin_command_line(0, 0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
