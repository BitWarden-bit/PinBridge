#include "pinbridge/pinbridge.h"

#include "pin_backend.h"

#include <cstdlib>
#include <cstring>
#include <limits>

namespace
{

template< typename Function > PbStatus GuardStatus(Function function)
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

uint32_t PB_CALL pb_abi_version(void) { return PB_ABI_VERSION; }

PbStatus PB_CALL pb_pin_version(char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        const char* version = PbGetBackend().version();
        if (!version)
            return PB_ERR_INTERNAL;
        const size_t length = std::strlen(version);
        const uint64_t required = static_cast<uint64_t>(length) + 1u;
        *required_size = required;
        if (!buffer || capacity < required)
            return PB_ERR_BUFFER_TOO_SMALL;
        std::memcpy(buffer, version, length + 1u);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_init(int32_t argc, char** argv)
{
    if (argc < 0 || (argc > 0 && !argv))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        return PbGetBackend().init(argc, argv) ? PB_ERR_PIN_REJECTED_ARGUMENTS : PB_OK;
    });
}

void PB_CALL pb_pin_start_program_default(void)
{
    PbGetBackend().start_program_default();
    std::abort();
}

PbStatus PB_CALL pb_ins_add_instrument_function(
    PbInsInstrumentCallback callback,
    void* user_data,
    PbCallbackHandle* out_callback)
{
    if (!callback || !out_callback)
        return PB_ERR_INVALID_ARGUMENT;
    out_callback->opaque = 0;
    return GuardStatus([&]() -> PbStatus {
        return PbGetBackend().add_ins_instrument_function(callback, user_data, &out_callback->opaque);
    });
}

PbStatus PB_CALL pb_pin_get_context_reg(PbConstContextHandle context, PbRegId reg, uint64_t* out_value)
{
    if (!context || !out_value)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        *out_value = PbGetBackend().get_context_reg(context, reg);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_safe_copy(
    void* destination,
    uint64_t source_address,
    uint64_t size,
    uint64_t* out_copied)
{
    if (!out_copied || (size != 0 && !destination))
        return PB_ERR_INVALID_ARGUMENT;
    if (size > static_cast<uint64_t>(std::numeric_limits<size_t>::max()))
        return PB_ERR_UNSUPPORTED;
    return GuardStatus([&]() -> PbStatus {
        *out_copied = PbGetBackend().safe_copy(destination, source_address, size);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_safe_copy_ex(
    void* destination,
    uint64_t source_address,
    uint64_t size,
    uint64_t* out_copied,
    PbExceptionInfoSnapshot* out_exception)
{
    if (!out_copied || !out_exception || (size != 0 && !destination))
        return PB_ERR_INVALID_ARGUMENT;
    if (size > static_cast<uint64_t>(std::numeric_limits<size_t>::max()))
        return PB_ERR_UNSUPPORTED;
    *out_copied = 0;
    std::memset(out_exception, 0, sizeof(*out_exception));
    return GuardStatus([&]() -> PbStatus {
        *out_copied = PbGetBackend().safe_copy_ex(
            destination, source_address, size, out_exception);
        return PB_OK;
    });
}
