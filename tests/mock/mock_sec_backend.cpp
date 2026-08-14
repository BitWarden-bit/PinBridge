#include "sec_backend.h"

#include <cstdio>
#include <cstring>

uint64_t PbBackendSecData(uint32_t sec)
{
    return UINT64_C(0x10000000) + sec;
}

int32_t PbBackendSecInvalid(void)
{
    return 0;
}

PbStatus PbBackendSecName(
    uint32_t sec, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    char value[32];
    const int length = std::snprintf(value, sizeof(value), "mock-section-%u", sec);
    if (length < 0 || static_cast<size_t>(length) >= sizeof(value))
        return PB_ERR_INTERNAL;
    *required_size = static_cast<uint64_t>(length) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value, static_cast<size_t>(*required_size));
    return PB_OK;
}
