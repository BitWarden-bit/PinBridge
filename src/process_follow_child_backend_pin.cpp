#include "pin.H"

#include "process_follow_child_backend.h"

#include <cstdlib>
#include <cstring>

namespace
{

struct FollowChildState
{
    PbFollowChildProcessCallback callback;
    void* user_data;
};

bool g_follow_child_registered;

BOOL OnFollowChild(CHILD_PROCESS child, VOID* raw_state)
{
    FollowChildState* state = static_cast<FollowChildState*>(raw_state);
    return state->callback(
        reinterpret_cast<PbChildProcessHandle>(child), state->user_data) != 0;
}

} // namespace

PbStatus PbBackendChildProcessGetCommandLineCount(
    PbChildProcessHandle child, int32_t* out_argc)
{
    INT argc = 0;
    const CHAR* const* argv = 0;
    CHILD_PROCESS_GetCommandLine(
        reinterpret_cast<CHILD_PROCESS>(child), &argc, &argv);
    *out_argc = static_cast<int32_t>(argc);
    return PB_OK;
}

PbStatus PbBackendChildProcessGetCommandLineArgument(
    PbChildProcessHandle child, int32_t index, char* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    INT argc = 0;
    const CHAR* const* argv = 0;
    CHILD_PROCESS_GetCommandLine(
        reinterpret_cast<CHILD_PROCESS>(child), &argc, &argv);
    if (index >= static_cast<int32_t>(argc) || !argv || !argv[index])
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = static_cast<uint64_t>(std::strlen(argv[index])) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, argv[index], static_cast<size_t>(*required_size));
    return PB_OK;
}

PbStatus PbBackendChildProcessGetId(
    PbChildProcessHandle child, uint32_t* out_process_id)
{
    *out_process_id = static_cast<uint32_t>(
        CHILD_PROCESS_GetId(reinterpret_cast<CHILD_PROCESS>(child)));
    return PB_OK;
}

PbStatus PbBackendChildProcessSetPinCommandLine(
    PbChildProcessHandle child, int32_t argc, const char* const* argv)
{
    CHILD_PROCESS_SetPinCommandLine(
        reinterpret_cast<CHILD_PROCESS>(child), static_cast<INT>(argc), argv);
    return PB_OK;
}

PbStatus PbBackendAddFollowChildProcessFunction(
    PbFollowChildProcessCallback callback, void* user_data, uint64_t* out_callback)
{
    if (g_follow_child_registered)
        return PB_ERR_INVALID_STATE;
    FollowChildState* state =
        static_cast<FollowChildState*>(std::malloc(sizeof(FollowChildState)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->callback = callback;
    state->user_data = user_data;
    const PIN_CALLBACK pin_callback =
        PIN_AddFollowChildProcessFunction(OnFollowChild, state);
    if (pin_callback == PIN_CALLBACK_INVALID)
    {
        std::free(state);
        return PB_ERR_INTERNAL;
    }
    g_follow_child_registered = true;
    *out_callback = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pin_callback));
    return PB_OK;
}
