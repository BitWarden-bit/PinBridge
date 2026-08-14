#include "utils_format_backend.h"

#include <algorithm>
#include <cstring>
#include <iomanip>
#include <sstream>
#include <string>

namespace
{
PbStatus CopyString(
    const std::string& value, char* buffer, uint64_t capacity,
    uint64_t* required_size);
}

PbStatus PbBackendReadLine(
    const char* input, uint64_t input_size, uint64_t offset,
    uint32_t line_number, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint64_t* out_next_offset,
    uint32_t* out_line_number)
{
    uint64_t cursor = offset;
    std::string value;
    while (cursor < input_size)
    {
        const uint64_t start = cursor;
        while (cursor < input_size && input[cursor] != '\n')
            ++cursor;
        value.assign(input + start, input + cursor);
        if (cursor < input_size)
            ++cursor;
        ++line_number;
        if (!value.empty() && value[0] != '#')
            break;
    }
    *out_next_offset = cursor;
    *out_line_number = line_number;
    return CopyString(value, buffer, capacity, required_size);
}

namespace
{

PbStatus CopyString(
    const std::string& value, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(value.size()) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}

std::string Pad(std::string value, uint32_t digits, char padding)
{
    if (value.size() < digits)
        value.insert(value.begin(), digits - value.size(), padding);
    return value;
}

std::string Hex(uint64_t value, uint32_t digits, bool prefix)
{
    std::ostringstream stream;
    if (prefix) stream << "0x";
    stream << std::hex << std::setfill('0') << std::setw(digits) << value;
    return stream.str();
}

std::string Bignum(int64_t value)
{
    std::string text = std::to_string(value);
    const size_t first = !text.empty() && text[0] == '-' ? 1u : 0u;
    for (size_t pos = text.size(); pos > first + 3u; pos -= 3u)
        text.insert(pos - 3u, 1u, ',');
    return text;
}

} // namespace

PbStatus PbBackendReformat(
    const char* text, const char* prefix, uint32_t, uint32_t,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(std::string(prefix) + text,
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringBignum(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Pad(Bignum(value), digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringBool(
    uint8_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(value ? "T" : "F", buffer, capacity, required_size);
}

PbStatus PbBackendStringDec(
    uint64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Pad(std::to_string(value), digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringDecSigned(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Pad(std::to_string(value), digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringFlt(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    std::ostringstream stream;
    stream << std::setw(width) << std::fixed << std::setprecision(precision) << value;
    return CopyString(stream.str(), buffer, capacity, required_size);
}

PbStatus PbBackendStringFromAddrint(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Hex(value, 16, true), buffer, capacity, required_size);
}

PbStatus PbBackendStringFromUint64(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Hex(value, 16, true), buffer, capacity, required_size);
}

PbStatus PbBackendStringHex(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Hex(value, digits, prefix_0x != 0),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringHex32(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return PbBackendStringHex(value, digits, prefix_0x,
                              buffer, capacity, required_size);
}

PbStatus PbBackendStringTri(
    PbTri value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    const char* text = value == PB_TRI_YES ? "Y" :
                       value == PB_TRI_NO ? "N" : "M";
    return CopyString(text, buffer, capacity, required_size);
}

PbStatus PbBackendLeftJustify(
    const char* text, uint32_t width, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    std::string value(text);
    if (value.size() < width)
        value.append(width - value.size(), padding);
    return CopyString(value, buffer, capacity, required_size);
}

PbStatus PbBackendPointerString(
    const void* pointer, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(Hex(reinterpret_cast<uintptr_t>(pointer), 16, true),
                      buffer, capacity, required_size);
}

#define PB_DEFINE_MOCK_DECSTR(function_name, value_type)                       \
    PbStatus function_name(                                                    \
        value_type value, uint32_t width, char* buffer, uint64_t capacity,     \
        uint64_t* required_size)                                               \
    {                                                                          \
        return CopyString(Pad(std::to_string(value), width, ' '),              \
                          buffer, capacity, required_size);                    \
    }

PB_DEFINE_MOCK_DECSTR(PbBackendDecstrI16, int16_t)
PB_DEFINE_MOCK_DECSTR(PbBackendDecstrI32, int32_t)
PB_DEFINE_MOCK_DECSTR(PbBackendDecstrI64, int64_t)
PB_DEFINE_MOCK_DECSTR(PbBackendDecstrU16, uint16_t)
PB_DEFINE_MOCK_DECSTR(PbBackendDecstrU32, uint32_t)
PB_DEFINE_MOCK_DECSTR(PbBackendDecstrU64, uint64_t)

#undef PB_DEFINE_MOCK_DECSTR

#define PB_DEFINE_MOCK_HEXSTR(function_name, value_type, conversion)           \
    PbStatus function_name(                                                    \
        value_type value, uint32_t width, char* buffer, uint64_t capacity,     \
        uint64_t* required_size)                                               \
    {                                                                          \
        return CopyString(Hex(conversion, width, false),                       \
                          buffer, capacity, required_size);                    \
    }

PB_DEFINE_MOCK_HEXSTR(
    PbBackendHexstrI16, int16_t, static_cast<uint32_t>(value))
PB_DEFINE_MOCK_HEXSTR(
    PbBackendHexstrI32, int32_t, static_cast<uint32_t>(value))
PB_DEFINE_MOCK_HEXSTR(
    PbBackendHexstrI64, int64_t, static_cast<uint64_t>(value))
PB_DEFINE_MOCK_HEXSTR(PbBackendHexstrU16, uint16_t, value)
PB_DEFINE_MOCK_HEXSTR(PbBackendHexstrU32, uint32_t, value)
PB_DEFINE_MOCK_HEXSTR(PbBackendHexstrU64, uint64_t, value)
PB_DEFINE_MOCK_HEXSTR(
    PbBackendHexstrPointer, void*, reinterpret_cast<uintptr_t>(value))
PB_DEFINE_MOCK_HEXSTR(
    PbBackendHexstrConstPointer, const void*,
    reinterpret_cast<uintptr_t>(value))

#undef PB_DEFINE_MOCK_HEXSTR

PbStatus PbBackendFltstr(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return PbBackendStringFlt(
        value, precision, width, buffer, capacity, required_size);
}
