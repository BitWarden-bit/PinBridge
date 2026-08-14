#include "debug_info_backend.h"

#include <cstring>

PbStatus PbBackendGetSourceLocation(
    uint64_t, int32_t* column, int32_t* line,
    char* file_name, uint64_t capacity, uint64_t* required_size)
{
    const char value[] = "mock/source.c";
    if (column)
        *column = 7;
    if (line)
        *line = 42;
    *required_size = sizeof(value);
    if (!file_name || capacity < sizeof(value))
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(file_name, value, sizeof(value));
    return PB_OK;
}
