#include "pin.H"

#include "inst_args_backend.h"
#include "rtn_varargs_backend.h"

namespace
{

RTN ToRtn(PbRtnHandle routine)
{
    RTN result;
    result.q_set(routine.opaque);
    return result;
}

AFUNPTR ToFunction(uint64_t address)
{
    return reinterpret_cast<AFUNPTR>(static_cast<uintptr_t>(address));
}

IARGLIST ToArguments(PbIargListHandle arguments)
{
    return static_cast<IARGLIST>(PbBackendIargListNative(arguments));
}

PbStatus StoreOriginal(AFUNPTR original, uint64_t* out_original)
{
    if (!original)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original = static_cast<uint64_t>(reinterpret_cast<uintptr_t>(original));
    return PB_OK;
}

} // namespace

PbStatus PbBackendRtnInsertCall(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    RTN_InsertCall(ToRtn(routine), static_cast<IPOINT>(point),
        ToFunction(callback_address), IARG_IARGLIST, ToArguments(arguments),
        IARG_END);
    return PB_OK;
}

PbStatus PbBackendRtnInsertCallProbed(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments, uint8_t* out_inserted)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_inserted = RTN_InsertCallProbed(
        ToRtn(routine), static_cast<IPOINT>(point), ToFunction(callback_address),
        IARG_IARGLIST, ToArguments(arguments), IARG_END) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendRtnInsertCallProbedEx(
    PbRtnHandle routine, PbIpoint point, PbProbeMode mode,
    uint64_t callback_address, PbIargListHandle arguments,
    uint8_t* out_inserted)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_inserted = RTN_InsertCallProbedEx(
        ToRtn(routine), static_cast<IPOINT>(point),
        static_cast<PROBE_MODE>(mode), ToFunction(callback_address),
        IARG_IARGLIST, ToArguments(arguments), IARG_END) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendRtnReplaceSignature(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    return StoreOriginal(RTN_ReplaceSignature(
        ToRtn(routine), ToFunction(replacement_address),
        IARG_IARGLIST, ToArguments(arguments), IARG_END), out_original);
}

PbStatus PbBackendRtnReplaceSignatureProbed(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    return StoreOriginal(RTN_ReplaceSignatureProbed(
        ToRtn(routine), ToFunction(replacement_address),
        IARG_IARGLIST, ToArguments(arguments), IARG_END), out_original);
}

PbStatus PbBackendRtnReplaceSignatureProbedEx(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    return StoreOriginal(RTN_ReplaceSignatureProbedEx(
        ToRtn(routine), static_cast<PROBE_MODE>(mode),
        ToFunction(replacement_address), IARG_IARGLIST,
        ToArguments(arguments), IARG_END), out_original);
}
