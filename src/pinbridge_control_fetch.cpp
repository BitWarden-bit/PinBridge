#include "pinbridge/pinbridge.h"

#include "control_fetch_backend.h"

namespace
{

template< typename Function > PbStatus GuardFetch(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_add_fetch_function(PbFetchCallback callback, void* user_data)
{
    if (!callback)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardFetch([&]() { return PbBackendAddFetchFunction(callback, user_data); });
}

PbStatus PB_CALL pb_pin_fetch_code(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info, uint64_t* out_copied)
{
    if (!out_copied || (max_size != 0 && (!copy_buffer || address == 0)))
        return PB_ERR_INVALID_ARGUMENT;
    *out_copied = 0;
    return GuardFetch([&]() -> PbStatus {
        *out_copied = PbBackendFetchCode(copy_buffer, address, max_size, exception_info);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_fetch_original_code(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info, uint64_t* out_copied)
{
    if (!out_copied || (max_size != 0 && !copy_buffer))
        return PB_ERR_INVALID_ARGUMENT;
    *out_copied = 0;
    return GuardFetch([&]() -> PbStatus {
        *out_copied = PbBackendFetchOriginalCode(
            copy_buffer, address, max_size, exception_info);
        return PB_OK;
    });
}
