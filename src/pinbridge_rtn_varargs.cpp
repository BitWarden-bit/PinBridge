#include "pinbridge/pinbridge.h"

#include "rtn_varargs_backend.h"

namespace
{

template< typename Function > PbStatus GuardRtnVarargs(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsCommonValid(
    PbRtnHandle routine, uint64_t function_address,
    PbIargListHandle arguments)
{
    return routine.opaque > 0 && function_address != 0 &&
        arguments != PB_IARG_LIST_INVALID;
}

bool IsPointValid(PbIpoint point)
{
    return point == PB_IPOINT_BEFORE || point == PB_IPOINT_AFTER;
}

bool IsModeValid(PbProbeMode mode)
{
    return (mode & ~(PB_PROBE_MODE_ALLOW_RELOCATION |
                     PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET)) == 0;
}

} // namespace

PbStatus PB_CALL pb_rtn_insert_call(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments)
{
    if (!IsCommonValid(routine, callback_address, arguments) ||
        !IsPointValid(point))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnInsertCall(
            routine, point, callback_address, arguments);
    });
}

PbStatus PB_CALL pb_rtn_insert_call_probed(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments, uint8_t* out_inserted)
{
    if (out_inserted)
        *out_inserted = 0;
    if (!out_inserted || !IsCommonValid(routine, callback_address, arguments) ||
        !IsPointValid(point))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnInsertCallProbed(
            routine, point, callback_address, arguments, out_inserted);
    });
}

PbStatus PB_CALL pb_rtn_insert_call_probed_ex(
    PbRtnHandle routine, PbIpoint point, PbProbeMode mode,
    uint64_t callback_address, PbIargListHandle arguments,
    uint8_t* out_inserted)
{
    if (out_inserted)
        *out_inserted = 0;
    if (!out_inserted || !IsModeValid(mode) ||
        !IsCommonValid(routine, callback_address, arguments) ||
        !IsPointValid(point))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnInsertCallProbedEx(
            routine, point, mode, callback_address, arguments, out_inserted);
    });
}

PbStatus PB_CALL pb_rtn_replace_signature(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address)
{
    if (out_original_address)
        *out_original_address = 0;
    if (!out_original_address ||
        !IsCommonValid(routine, replacement_address, arguments))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnReplaceSignature(
            routine, replacement_address, arguments, out_original_address);
    });
}

PbStatus PB_CALL pb_rtn_replace_signature_probed(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address)
{
    if (out_original_address)
        *out_original_address = 0;
    if (!out_original_address ||
        !IsCommonValid(routine, replacement_address, arguments))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnReplaceSignatureProbed(
            routine, replacement_address, arguments, out_original_address);
    });
}

PbStatus PB_CALL pb_rtn_replace_signature_probed_ex(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address)
{
    if (out_original_address)
        *out_original_address = 0;
    if (!out_original_address || !IsModeValid(mode) ||
        !IsCommonValid(routine, replacement_address, arguments))
        return PB_ERR_INVALID_ARGUMENT;
    return GuardRtnVarargs([&]() {
        return PbBackendRtnReplaceSignatureProbedEx(
            routine, mode, replacement_address, arguments,
            out_original_address);
    });
}
