#include "pin.H"

#include "sec_backend.h"

#include <cstring>
#include <string>

namespace
{

SEC ToSec(uint32_t value)
{
    SEC result;
    result.q_set(static_cast<int32_t>(value));
    return result;
}

} // namespace

uint64_t PbBackendSecData(uint32_t sec)
{
    return static_cast<uint64_t>(
        reinterpret_cast<uintptr_t>(SEC_Data(ToSec(sec))));
}

int32_t PbBackendSecInvalid(void)
{
    return SEC_Invalid().q();
}

PbStatus PbBackendSecName(
    uint32_t sec, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    const std::string& value = SEC_Name(ToSec(sec));
    *required_size = static_cast<uint64_t>(value.size()) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}
