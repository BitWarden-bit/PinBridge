#ifndef PINBRIDGE_UTILS_FORMAT_BACKEND_H
#define PINBRIDGE_UTILS_FORMAT_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendReformat(
    const char* text, const char* prefix, uint32_t min_line, uint32_t max_line,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringBignum(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringBool(
    uint8_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringDec(
    uint64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringDecSigned(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringFlt(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringFromAddrint(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringFromUint64(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringHex(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringHex32(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendStringTri(
    PbTri value, char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendLeftJustify(
    const char* text, uint32_t width, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendPointerString(
    const void* pointer, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PbStatus PbBackendReadLine(
    const char* input, uint64_t input_size, uint64_t offset,
    uint32_t line_number, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint64_t* out_next_offset,
    uint32_t* out_line_number);
PbStatus PbBackendDecstrI16(
    int16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendDecstrI32(
    int32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendDecstrI64(
    int64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendDecstrU16(
    uint16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendDecstrU32(
    uint32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendDecstrU64(
    uint64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendFltstr(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrI16(
    int16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrI32(
    int32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrI64(
    int64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrU16(
    uint16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrU32(
    uint32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrU64(
    uint64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrPointer(
    void* pointer, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PbStatus PbBackendHexstrConstPointer(
    const void* pointer, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);

#endif
