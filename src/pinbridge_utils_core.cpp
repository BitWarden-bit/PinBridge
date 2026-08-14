#include "pinbridge/pinbridge.h"

#include "utils_core_backend.h"

namespace
{

template< typename Function > PbStatus GuardUtils(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

template< typename T, typename Function >
PbStatus Scalar(T* output, Function function)
{
    if (!output)
        return PB_ERR_INVALID_ARGUMENT;
    *output = T();
    return GuardUtils([&]() -> PbStatus {
        *output = static_cast<T>(function());
        return PB_OK;
    });
}

template< typename T, typename Function >
PbStatus Parsed(const char* text, T* output, Function function)
{
    if (!text)
        return PB_ERR_INVALID_ARGUMENT;
    return Scalar(output, [&]() { return function(text); });
}

} // namespace

PbStatus PB_CALL pb_addrint_to_pointer(uint64_t address, void** out_pointer)
{ return Scalar(out_pointer, [&]() { return PbBackendAddrintToPointer(address); }); }
PbStatus PB_CALL pb_addrint_from_string(const char* text, uint64_t* out_value)
{ return Parsed(text, out_value, PbBackendAddrintFromString); }
PbStatus PB_CALL pb_bit_count(uint64_t value, uint32_t* out_count)
{ return Scalar(out_count, [&]() { return PbBackendBitCount(value); }); }
PbStatus PB_CALL pb_char_is_space(char value, uint8_t* out_is_space)
{ return Scalar(out_is_space, [&]() { return PbBackendCharIsSpace(value); }); }
PbStatus PB_CALL pb_char_to_hex_digit(char value, int32_t* out_digit)
{ return Scalar(out_digit, [&]() { return PbBackendCharToHexDigit(value); }); }
PbStatus PB_CALL pb_char_to_upper(char value, char* out_upper)
{ return Scalar(out_upper, [&]() { return PbBackendCharToUpper(value); }); }
PbStatus PB_CALL pb_flt64_from_string(const char* text, double* out_value)
{ return Parsed(text, out_value, PbBackendFlt64FromString); }
PbStatus PB_CALL pb_get_page_of_addr(uint64_t address, uint64_t* out_page)
{ return Scalar(out_page, [&]() { return PbBackendGetPageOfAddr(address); }); }
PbStatus PB_CALL pb_mem_page_range_addr(
    uint64_t address, PbMemRange* out_range)
{
    if (!out_range)
        return PB_ERR_INVALID_ARGUMENT;
    *out_range = PbMemRange();
    return GuardUtils([&]() -> PbStatus {
        PbBackendMemPageRangeAddr(address, out_range);
        return PB_OK;
    });
}
PbStatus PB_CALL pb_mem_page_range_pointer(
    const void* pointer, PbMemRange* out_range)
{
    if (!out_range)
        return PB_ERR_INVALID_ARGUMENT;
    *out_range = PbMemRange();
    return GuardUtils([&]() -> PbStatus {
        PbBackendMemPageRangePointer(pointer, out_range);
        return PB_OK;
    });
}
PbStatus PB_CALL pb_get_sp(const void** out_stack_pointer)
{ return Scalar(out_stack_pointer, PbBackendGetSp); }
PbStatus PB_CALL pb_int32_from_string(const char* text, int32_t* out_value)
{ return Parsed(text, out_value, PbBackendInt32FromString); }
PbStatus PB_CALL pb_int64_from_string(const char* text, int64_t* out_value)
{ return Parsed(text, out_value, PbBackendInt64FromString); }

PbStatus PB_CALL pb_ptr_at_offset(
    void* pointer, uint64_t offset, void** out_pointer)
{
    if (!pointer)
        return PB_ERR_INVALID_ARGUMENT;
    return Scalar(out_pointer,
        [&]() { return PbBackendPtrAtOffset(pointer, offset); });
}

PbStatus PB_CALL pb_const_ptr_at_offset(
    const void* pointer, uint64_t offset, const void** out_pointer)
{
    if (!pointer)
        return PB_ERR_INVALID_ARGUMENT;
    return Scalar(out_pointer,
        [&]() { return PbBackendConstPtrAtOffset(pointer, offset); });
}

PbStatus PB_CALL pb_ptr_at_offset_typed(
    void* pointer, uint64_t offset, void** out_pointer)
{
    return pb_ptr_at_offset(pointer, offset, out_pointer);
}

PbStatus PB_CALL pb_const_ptr_at_offset_typed(
    const void* pointer, uint64_t offset, const void** out_pointer)
{
    return pb_const_ptr_at_offset(pointer, offset, out_pointer);
}

PbStatus PB_CALL pb_ptr_diff(
    const void* pointer1, const void* pointer2, uint64_t* out_difference)
{
    if (!pointer1 || !pointer2)
        return PB_ERR_INVALID_ARGUMENT;
    return Scalar(out_difference,
        [&]() { return PbBackendPtrDiff(pointer1, pointer2); });
}

PbStatus PB_CALL pb_uint32_from_string(const char* text, uint32_t* out_value)
{ return Parsed(text, out_value, PbBackendUint32FromString); }
PbStatus PB_CALL pb_uint64_from_string(const char* text, uint64_t* out_value)
{ return Parsed(text, out_value, PbBackendUint64FromString); }
PbStatus PB_CALL pb_pointer_to_addrint(void* pointer, uint64_t* out_address)
{ return Scalar(out_address, [&]() { return PbBackendPointerToAddrint(pointer); }); }
PbStatus PB_CALL pb_const_pointer_to_addrint(
    const void* pointer, uint64_t* out_address)
{ return Scalar(out_address,
    [&]() { return PbBackendConstPointerToAddrint(pointer); }); }
PbStatus PB_CALL pb_round_down_addr(
    uint64_t address, uint64_t alignment, uint64_t* out_address)
{ return Scalar(out_address,
    [&]() { return PbBackendRoundDownAddr(address, alignment); }); }
PbStatus PB_CALL pb_round_down_u64(
    uint64_t value, uint64_t alignment, uint64_t* out_value)
{ return Scalar(out_value,
    [&]() { return PbBackendRoundDownU64(value, alignment); }); }
PbStatus PB_CALL pb_round_up_addr(
    uint64_t address, uint64_t alignment, uint64_t* out_address)
{ return Scalar(out_address,
    [&]() { return PbBackendRoundUpAddr(address, alignment); }); }
PbStatus PB_CALL pb_round_up_u64(
    uint64_t value, uint64_t alignment, uint64_t* out_value)
{ return Scalar(out_value,
    [&]() { return PbBackendRoundUpU64(value, alignment); }); }
