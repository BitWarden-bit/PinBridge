#include "pinbridge/pinbridge.h"

#include "structure_query_backend.h"

namespace
{

template< typename Function > PbStatus GuardQuery(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

template< typename T > void StoreResult(uint64_t bits, T* output)
{
    *output = static_cast<T>(bits);
}

#define PB_DEFINE_HANDLE_STORE(type) \
    void StoreResult(uint64_t bits, type* output) { output->opaque = static_cast<int32_t>(bits); }
PB_DEFINE_HANDLE_STORE(PbInsHandle)
PB_DEFINE_HANDLE_STORE(PbBblHandle)
PB_DEFINE_HANDLE_STORE(PbRtnHandle)
PB_DEFINE_HANDLE_STORE(PbSecHandle)
PB_DEFINE_HANDLE_STORE(PbImgHandle)
PB_DEFINE_HANDLE_STORE(PbSymHandle)
#undef PB_DEFINE_HANDLE_STORE

#define PB_INPUT_VALID_BBL(value) ((value).opaque > 0)
#define PB_INPUT_VALID_TRACE(value) ((value) != 0)
#define PB_INPUT_VALID_RTN(value) ((value).opaque > 0)
#define PB_INPUT_VALID_SEC(value) ((value).opaque > 0)
#define PB_INPUT_VALID_IMG(value) ((value).opaque > 0)
#define PB_INPUT_RAW_BBL(value) static_cast<uint32_t>((value).opaque)
#define PB_INPUT_RAW_TRACE(value) static_cast<uint64_t>(reinterpret_cast<uintptr_t>(value))
#define PB_INPUT_RAW_RTN(value) static_cast<uint32_t>((value).opaque)
#define PB_INPUT_RAW_SEC(value) static_cast<uint32_t>((value).opaque)
#define PB_INPUT_RAW_IMG(value) static_cast<uint32_t>((value).opaque)

template< typename T > PbStatus Query(
    uint32_t query_id, uint64_t input, uint64_t argument, T* output)
{
    if (!output)
        return PB_ERR_INVALID_ARGUMENT;
#if defined(_WIN32)
    if (query_id == PB_STRUCTURE_QUERY_ID_pb_rtn_i_func_implementation ||
        query_id == PB_STRUCTURE_QUERY_ID_pb_rtn_i_func_resolver)
        return PB_ERR_UNSUPPORTED;
#endif
    return GuardQuery([&]() -> PbStatus {
        StoreResult(PbBackendStructureQuery(query_id, input, argument), output);
        return PB_OK;
    });
}

bool IsValidProbeMode(PbProbeMode mode)
{
    return (mode & ~(PB_PROBE_MODE_ALLOW_RELOCATION |
                     PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET)) == 0;
}

template< typename Function >
PbStatus ReplaceRoutine(
    PbRtnHandle routine, uint64_t replacement_address,
    uint64_t* out_original_address, Function function)
{
    if (out_original_address)
        *out_original_address = 0;
    if (routine.opaque <= 0 || replacement_address == 0 ||
        !out_original_address)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() {
        return function(
            routine.opaque, replacement_address, out_original_address);
    });
}

} // namespace

#define PB_HANDLE_QUERY0(input_kind, return_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol( \
        PB_HANDLE_C_INPUT_##input_kind input, PB_HANDLE_C_TYPE_##return_kind* out_value) \
    { \
        if (!PB_INPUT_VALID_##input_kind(input)) return PB_ERR_INVALID_ARGUMENT; \
        return Query(PB_STRUCTURE_QUERY_ID_##c_symbol, PB_INPUT_RAW_##input_kind(input), 0, out_value); \
    }
#define PB_HANDLE_QUERY1(input_kind, return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PbStatus PB_CALL c_symbol( \
        PB_HANDLE_C_INPUT_##input_kind input, PB_HANDLE_C_ARG_##argument_kind argument, \
        PB_HANDLE_C_TYPE_##return_kind* out_value) \
    { \
        if (!PB_INPUT_VALID_##input_kind(input)) return PB_ERR_INVALID_ARGUMENT; \
        return Query(PB_STRUCTURE_QUERY_ID_##c_symbol, PB_INPUT_RAW_##input_kind(input), \
                     static_cast<uint64_t>(argument), out_value); \
    }
#include "pinbridge/generated/structure_queries.inc"
#undef PB_HANDLE_QUERY1
#undef PB_HANDLE_QUERY0

PbStatus PB_CALL pb_rtn_close(PbRtnHandle routine)
{
    if (routine.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() { return PbBackendRtnClose(routine.opaque); });
}

PbStatus PB_CALL pb_rtn_create_at(
    uint64_t address, const char* name, PbRtnHandle* out_routine)
{
    if (!name || name[0] == '\0' || !out_routine)
        return PB_ERR_INVALID_ARGUMENT;
    out_routine->opaque = 0;
    return GuardQuery([&]() {
        return PbBackendRtnCreateAt(address, name, &out_routine->opaque);
    });
}

PbStatus PB_CALL pb_rtn_find_by_address(
    uint64_t address, PbRtnHandle* out_routine)
{
    if (!out_routine)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        out_routine->opaque = PbBackendRtnFindByAddress(address);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_rtn_find_by_name(
    PbImgHandle image, const char* name, PbRtnHandle* out_routine)
{
    if (image.opaque <= 0 || !name || name[0] == '\0' || !out_routine)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        out_routine->opaque = PbBackendRtnFindByName(image.opaque, name);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_rtn_find_name_by_address(
    uint64_t address, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    if (!required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() {
        return PbBackendRtnFindNameByAddress(
            address, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_rtn_funptr(
    PbRtnHandle routine, uint64_t* out_function_address)
{
    if (routine.opaque <= 0 || !out_function_address)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        *out_function_address = PbBackendRtnFunptr(routine.opaque);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_rtn_invalid(PbRtnHandle* out_routine)
{
    if (!out_routine)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() -> PbStatus {
        out_routine->opaque = PbBackendRtnInvalid();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_rtn_name(
    PbRtnHandle routine, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    if (routine.opaque <= 0 || !required_size || (!buffer && capacity != 0))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() {
        return PbBackendRtnName(
            routine.opaque, buffer, capacity, required_size);
    });
}

PbStatus PB_CALL pb_rtn_open(PbRtnHandle routine)
{
    if (routine.opaque <= 0)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() { return PbBackendRtnOpen(routine.opaque); });
}

PbStatus PB_CALL pb_rtn_replace(
    PbRtnHandle routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    return ReplaceRoutine(
        routine, replacement_address, out_original_address,
        PbBackendRtnReplace);
}

PbStatus PB_CALL pb_rtn_replace_probed(
    PbRtnHandle routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    return ReplaceRoutine(
        routine, replacement_address, out_original_address,
        PbBackendRtnReplaceProbed);
}

PbStatus PB_CALL pb_rtn_replace_probed_ex(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (!IsValidProbeMode(mode))
    {
        if (out_original_address)
            *out_original_address = 0;
        return PB_ERR_INVALID_ARGUMENT;
    }
    if (out_original_address)
        *out_original_address = 0;
    if (routine.opaque <= 0 || replacement_address == 0 ||
        !out_original_address)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardQuery([&]() {
        return PbBackendRtnReplaceProbedEx(
            routine.opaque, mode, replacement_address,
            out_original_address);
    });
}
