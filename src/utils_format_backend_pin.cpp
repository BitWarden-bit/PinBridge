#include "pin.H"

#include "utils_format_backend.h"

#include <cstring>
#include <sstream>
#include <string>

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

} // namespace

static_assert(sizeof(ADDRINT) == sizeof(uint64_t), "ADDRINT is not 64-bit");
static_assert(sizeof(FLT64) == sizeof(double), "FLT64 is not a C double");
static_assert(sizeof(INT16) == sizeof(int16_t), "INT16 width changed");
static_assert(sizeof(INT32) == sizeof(int32_t), "INT32 width changed");
static_assert(sizeof(INT64) == sizeof(int64_t), "INT64 width changed");
static_assert(sizeof(UINT16) == sizeof(uint16_t), "UINT16 width changed");
static_assert(sizeof(UINT32) == sizeof(uint32_t), "UINT32 width changed");
static_assert(sizeof(UINT64) == sizeof(uint64_t), "UINT64 width changed");
static_assert(PB_TRI_YES == TRI_YES, "TRI_YES value changed");
static_assert(PB_TRI_NO == TRI_NO, "TRI_NO value changed");
static_assert(PB_TRI_MAYBE == TRI_MAYBE, "TRI_MAYBE value changed");

PbStatus PbBackendReadLine(
    const char* input, uint64_t input_size, uint64_t offset,
    uint32_t line_number, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint64_t* out_next_offset,
    uint32_t* out_line_number)
{
    std::istringstream stream(std::string(input, static_cast<size_t>(input_size)));
    stream.seekg(static_cast<std::streamoff>(offset));
    UINT32 current_line = line_number;
    const std::string value = ReadLine(stream, &current_line);
    const std::streampos position = stream.tellg();
    *out_next_offset = position == std::streampos(-1) ? input_size :
        static_cast<uint64_t>(position);
    *out_line_number = current_line;
    return CopyString(value, buffer, capacity, required_size);
}

PbStatus PbBackendReformat(
    const char* text, const char* prefix, uint32_t min_line, uint32_t max_line,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(Reformat(std::string(text), std::string(prefix),
        min_line, max_line), buffer, capacity, required_size);
}

PbStatus PbBackendStringBignum(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringBignum(value, digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringBool(
    uint8_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringBool(value != 0), buffer, capacity, required_size);
}

PbStatus PbBackendStringDec(
    uint64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringDec(value, digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringDecSigned(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringDecSigned(value, digits, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringFlt(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringFlt(value, precision, width),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringFromAddrint(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringFromAddrint(static_cast<ADDRINT>(value)),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringFromUint64(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringFromUint64(value), buffer, capacity, required_size);
}

PbStatus PbBackendStringHex(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringHex(value, digits, prefix_0x != 0),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringHex32(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringHex32(value, digits, prefix_0x != 0),
                      buffer, capacity, required_size);
}

PbStatus PbBackendStringTri(
    PbTri value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(StringTri(static_cast<TRI>(value)),
                      buffer, capacity, required_size);
}

PbStatus PbBackendLeftJustify(
    const char* text, uint32_t width, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(ljstr(std::string(text), width, padding),
                      buffer, capacity, required_size);
}

PbStatus PbBackendPointerString(
    const void* pointer, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(ptrstr(pointer), buffer, capacity, required_size);
}

#define PB_DEFINE_PIN_LEGACY_FORMAT(function_name, value_type, pin_type, pin_function) \
    PbStatus function_name(                                                     \
        value_type value, uint32_t width, char* buffer, uint64_t capacity,      \
        uint64_t* required_size)                                                \
    {                                                                           \
        return CopyString(pin_function(static_cast<pin_type>(value), width),    \
                          buffer, capacity, required_size);                     \
    }

PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrI16, int16_t, INT16, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrI32, int32_t, INT32, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrI64, int64_t, INT64, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrU16, uint16_t, UINT16, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrU32, uint32_t, UINT32, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendDecstrU64, uint64_t, UINT64, decstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrI16, int16_t, INT16, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrI32, int32_t, INT32, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrI64, int64_t, INT64, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrU16, uint16_t, UINT16, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrU32, uint32_t, UINT32, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrU64, uint64_t, UINT64, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(PbBackendHexstrPointer, void*, void*, hexstr)
PB_DEFINE_PIN_LEGACY_FORMAT(
    PbBackendHexstrConstPointer, const void*, const void*, hexstr)

#undef PB_DEFINE_PIN_LEGACY_FORMAT

PbStatus PbBackendFltstr(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return CopyString(fltstr(value, precision, width),
                      buffer, capacity, required_size);
}
