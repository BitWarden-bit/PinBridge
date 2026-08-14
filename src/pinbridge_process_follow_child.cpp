#include "pinbridge/pinbridge.h"

#include "process_follow_child_backend.h"

namespace
{

template< typename Function > PbStatus GuardCall(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_child_process_get_command_line_count(
    PbChildProcessHandle child, int32_t* out_argc)
{
    if (!child || !out_argc)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardCall([&]() {
        return PbBackendChildProcessGetCommandLineCount(child, out_argc);
    });
}

PbStatus PB_CALL pb_child_process_get_command_line_argument(
    PbChildProcessHandle child, int32_t index, char* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    if (!child || index < 0 || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardCall([&]() {
        return PbBackendChildProcessGetCommandLineArgument(
            child, index, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_child_process_get_id(
    PbChildProcessHandle child, uint32_t* out_process_id)
{
    if (!child || !out_process_id)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardCall([&]() {
        return PbBackendChildProcessGetId(child, out_process_id);
    });
}

PbStatus PB_CALL pb_child_process_set_pin_command_line(
    PbChildProcessHandle child, int32_t argc, const char* const* argv)
{
    if (!child || argc < 0 || (argc != 0 && !argv))
        return PB_ERR_INVALID_ARGUMENT;
    for (int32_t index = 0; index < argc; ++index)
    {
        if (!argv[index])
            return PB_ERR_INVALID_ARGUMENT;
    }
    return GuardCall([&]() {
        return PbBackendChildProcessSetPinCommandLine(child, argc, argv);
    });
}

PbStatus PB_CALL pb_pin_add_follow_child_process_function(
    PbFollowChildProcessCallback callback, void* user_data,
    PbCallbackHandle* out_callback)
{
    if (out_callback)
        out_callback->opaque = 0;
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardCall([&]() {
        return PbBackendAddFollowChildProcessFunction(
            callback, user_data, &out_callback->opaque);
    });
}
