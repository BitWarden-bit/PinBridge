#include "pinbridge/pinbridge.h"

#include "control_query_backend.h"

#include <cstring>

namespace
{

template< typename Function > PbStatus Invoke(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_check_read_access(uint64_t address, uint8_t* out_accessible)
{
    if (!out_accessible)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendCheckReadAccess(address, out_accessible); });
}

PbStatus PB_CALL pb_pin_check_write_access(uint64_t address, uint8_t* out_accessible)
{
    if (!out_accessible)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendCheckWriteAccess(address, out_accessible); });
}

PbStatus PB_CALL pb_pin_is_attaching(uint8_t* out_attaching)
{
    if (!out_attaching)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendIsAttaching(out_attaching); });
}

PbStatus PB_CALL pb_pin_is_probe_mode(uint8_t* out_probe_mode)
{
    if (!out_probe_mode)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendIsProbeMode(out_probe_mode); });
}

PbStatus PB_CALL pb_pin_is_safe_for_probed_insertion(uint64_t address, uint8_t* out_safe)
{
    if (!out_safe)
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() { return PbBackendIsSafeForProbedInsertion(address, out_safe); });
}

PbStatus PB_CALL pb_pin_tool_full_path(
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return Invoke([=]() -> PbStatus {
        const char* path = PbBackendToolFullPath();
        if (!path)
            return PB_ERR_INTERNAL;
        const uint64_t required = static_cast<uint64_t>(std::strlen(path)) + 1u;
        *required_size = required;
        if (!buffer || capacity < required)
            return PB_ERR_BUFFER_TOO_SMALL;
        std::memcpy(buffer, path, static_cast<size_t>(required));
        return PB_OK;
    });
}
