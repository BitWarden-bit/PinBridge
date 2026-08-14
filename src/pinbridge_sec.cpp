#include "pinbridge/pinbridge.h"

#include "sec_backend.h"

namespace
{

template< typename Function > PbStatus GuardSec(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsValid(PbSecHandle sec)
{
    return sec.opaque > 0;
}

} // namespace

PbStatus PB_CALL pb_sec_data(PbSecHandle sec, uint64_t* out_address)
{
    if (!IsValid(sec) || !out_address)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardSec([&]() -> PbStatus {
        *out_address = PbBackendSecData(static_cast<uint32_t>(sec.opaque));
        return PB_OK;
    });
}

PbStatus PB_CALL pb_sec_invalid(PbSecHandle* out_sec)
{
    if (!out_sec)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardSec([&]() -> PbStatus {
        out_sec->opaque = PbBackendSecInvalid();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_sec_name(
    PbSecHandle sec, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!IsValid(sec) || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardSec([&]() -> PbStatus {
        return PbBackendSecName(
            static_cast<uint32_t>(sec.opaque), buffer, capacity, required_size);
    });
}
