#include "rtn_varargs_backend.h"

namespace
{

bool Common(
    PbRtnHandle routine, uint64_t address, PbIargListHandle arguments)
{
    return routine.opaque == 91 && address >= 0x1000 &&
        arguments == reinterpret_cast<PbIargListHandle>(0x2000);
}

} // namespace

PbStatus PbBackendRtnInsertCall(
    PbRtnHandle routine, PbIpoint point, uint64_t address,
    PbIargListHandle arguments)
{
    return Common(routine, address, arguments) && point == PB_IPOINT_BEFORE ?
        PB_OK : PB_ERR_INTERNAL;
}

PbStatus PbBackendRtnInsertCallProbed(
    PbRtnHandle routine, PbIpoint point, uint64_t address,
    PbIargListHandle arguments, uint8_t* out_inserted)
{
    if (!Common(routine, address, arguments) || point != PB_IPOINT_AFTER)
        return PB_ERR_INTERNAL;
    *out_inserted = 1;
    return PB_OK;
}

PbStatus PbBackendRtnInsertCallProbedEx(
    PbRtnHandle routine, PbIpoint point, PbProbeMode mode, uint64_t address,
    PbIargListHandle arguments, uint8_t* out_inserted)
{
    if (!Common(routine, address, arguments) || point != PB_IPOINT_BEFORE ||
        mode != PB_PROBE_MODE_ALLOW_RELOCATION)
        return PB_ERR_INTERNAL;
    *out_inserted = 1;
    return PB_OK;
}

PbStatus PbBackendRtnReplaceSignature(
    PbRtnHandle routine, uint64_t address, PbIargListHandle arguments,
    uint64_t* out_original)
{
    if (!Common(routine, address, arguments)) return PB_ERR_INTERNAL;
    *out_original = 0x4000 + (address >> 4);
    return PB_OK;
}

PbStatus PbBackendRtnReplaceSignatureProbed(
    PbRtnHandle routine, uint64_t address, PbIargListHandle arguments,
    uint64_t* out_original)
{
    return PbBackendRtnReplaceSignature(routine, address, arguments, out_original);
}

PbStatus PbBackendRtnReplaceSignatureProbedEx(
    PbRtnHandle routine, PbProbeMode mode, uint64_t address,
    PbIargListHandle arguments, uint64_t* out_original)
{
    if (mode != PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET)
        return PB_ERR_INTERNAL;
    return PbBackendRtnReplaceSignature(routine, address, arguments, out_original);
}
