#include "pinbridge/pinbridge.h"

#include "utils_format_backend.h"

namespace
{

template< typename Function >
PbStatus Format(char* buffer, uint64_t capacity, uint64_t* required_size,
                Function function)
{
    if (!required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = 0;
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_read_line(
    const char* input, uint64_t input_size, uint64_t offset,
    uint32_t line_number, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint64_t* out_next_offset,
    uint32_t* out_line_number)
{
    if (required_size)
        *required_size = 0;
    if (out_next_offset)
        *out_next_offset = 0;
    if (out_line_number)
        *out_line_number = 0;
    if (!input || offset > input_size || !required_size || !out_next_offset ||
        !out_line_number || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendReadLine(input, input_size, offset, line_number,
            buffer, capacity, required_size, out_next_offset,
            out_line_number);
    });
}

PbStatus PB_CALL pb_reformat(
    const char* text, const char* prefix, uint32_t min_line, uint32_t max_line,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!text || !prefix || max_line == 0 || min_line > max_line)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendReformat(text, prefix, min_line, max_line,
                                 buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_bignum(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringBignum(value, digits, padding,
                                     buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_bool(
    uint8_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (value > 1)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringBool(value, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_dec(
    uint64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringDec(value, digits, padding,
                                  buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_dec_signed(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringDecSigned(value, digits, padding,
                                        buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_flt(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringFlt(value, precision, width,
                                  buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_from_addrint(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringFromAddrint(
            value, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_from_uint64(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringFromUint64(
            value, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_hex(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (prefix_0x > 1)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringHex(value, digits, prefix_0x,
                                  buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_hex32(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (prefix_0x > 1)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringHex32(value, digits, prefix_0x,
                                    buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_string_tri(
    PbTri value, char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (value < PB_TRI_YES || value > PB_TRI_MAYBE)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendStringTri(value, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_left_justify(
    const char* text, uint32_t width, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    if (!text)
        return PB_ERR_INVALID_ARGUMENT;
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendLeftJustify(text, width, padding,
                                    buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_pointer_string(
    const void* pointer, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendPointerString(
            pointer, buffer, capacity, required_size);
    });
}

#define PB_DEFINE_LEGACY_FORMAT(function_name, value_type, backend_name)       \
    PbStatus PB_CALL function_name(                                           \
        value_type value, uint32_t width, char* buffer, uint64_t capacity,    \
        uint64_t* required_size)                                              \
    {                                                                         \
        return Format(buffer, capacity, required_size, [&]() {                \
            return backend_name(                                              \
                value, width, buffer, capacity, required_size);               \
        });                                                                   \
    }

PB_DEFINE_LEGACY_FORMAT(pb_decstr_i16, int16_t, PbBackendDecstrI16)
PB_DEFINE_LEGACY_FORMAT(pb_decstr_i32, int32_t, PbBackendDecstrI32)
PB_DEFINE_LEGACY_FORMAT(pb_decstr_i64, int64_t, PbBackendDecstrI64)
PB_DEFINE_LEGACY_FORMAT(pb_decstr_u16, uint16_t, PbBackendDecstrU16)
PB_DEFINE_LEGACY_FORMAT(pb_decstr_u32, uint32_t, PbBackendDecstrU32)
PB_DEFINE_LEGACY_FORMAT(pb_decstr_u64, uint64_t, PbBackendDecstrU64)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_i16, int16_t, PbBackendHexstrI16)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_i32, int32_t, PbBackendHexstrI32)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_i64, int64_t, PbBackendHexstrI64)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_u16, uint16_t, PbBackendHexstrU16)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_u32, uint32_t, PbBackendHexstrU32)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_u64, uint64_t, PbBackendHexstrU64)
PB_DEFINE_LEGACY_FORMAT(pb_hexstr_pointer, void*, PbBackendHexstrPointer)
PB_DEFINE_LEGACY_FORMAT(
    pb_hexstr_const_pointer, const void*, PbBackendHexstrConstPointer)

#undef PB_DEFINE_LEGACY_FORMAT

PbStatus PB_CALL pb_fltstr(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size)
{
    return Format(buffer, capacity, required_size, [&]() {
        return PbBackendFltstr(value, precision, width,
                               buffer, capacity, required_size);
    });
}
