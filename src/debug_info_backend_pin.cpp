#include "pin.H"

#include "debug_info_backend.h"

#include <cstring>
#include <string>

PbStatus PbBackendGetSourceLocation(
    uint64_t address, int32_t* column, int32_t* line,
    char* file_name, uint64_t capacity, uint64_t* required_size)
{
    INT32 pin_column = 0;
    INT32 pin_line = 0;
    std::string pin_file_name;
    PIN_LockClient();
    PIN_GetSourceLocation(
        static_cast<ADDRINT>(address), &pin_column, &pin_line, &pin_file_name);
    PIN_UnlockClient();
    if (column)
        *column = static_cast<int32_t>(pin_column);
    if (line)
        *line = static_cast<int32_t>(pin_line);
    *required_size = static_cast<uint64_t>(pin_file_name.size()) + 1u;
    if (!file_name || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(file_name, pin_file_name.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}
