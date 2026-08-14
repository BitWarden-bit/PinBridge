#include "process_follow_child_backend.h"

#include <cstring>

namespace
{

const char* const kChildArguments[] = {"child.exe", "--payload", "value"};
bool g_pin_command_line_checked;
bool g_follow_child_registered;

} // namespace

PbStatus PbBackendChildProcessGetCommandLineCount(
    PbChildProcessHandle, int32_t* out_argc)
{
    *out_argc = 3;
    return PB_OK;
}

PbStatus PbBackendChildProcessGetCommandLineArgument(
    PbChildProcessHandle, int32_t index, char* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    if (index < 0 || index >= 3)
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = static_cast<uint64_t>(std::strlen(kChildArguments[index])) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, kChildArguments[index], static_cast<size_t>(*required_size));
    return PB_OK;
}

PbStatus PbBackendChildProcessGetId(
    PbChildProcessHandle, uint32_t* out_process_id)
{
    *out_process_id = UINT32_C(0x12345678);
    return PB_OK;
}

PbStatus PbBackendChildProcessSetPinCommandLine(
    PbChildProcessHandle, int32_t argc, const char* const* argv)
{
    g_pin_command_line_checked =
        argc == 4 && std::strcmp(argv[0], "pin.exe") == 0 &&
        std::strcmp(argv[1], "-t") == 0 &&
        std::strcmp(argv[2], "tool.dll") == 0 &&
        std::strcmp(argv[3], "--") == 0;
    return g_pin_command_line_checked ? PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendAddFollowChildProcessFunction(
    PbFollowChildProcessCallback callback, void* user_data, uint64_t* out_callback)
{
    if (g_follow_child_registered)
        return PB_ERR_INVALID_STATE;
    g_follow_child_registered = true;
    *out_callback = UINT64_C(0x4001);
    const uint8_t follow = callback(
        reinterpret_cast<PbChildProcessHandle>(static_cast<uintptr_t>(0x5000)),
        user_data);
    return follow != 0 && g_pin_command_line_checked ? PB_OK : PB_ERR_INTERNAL;
}
