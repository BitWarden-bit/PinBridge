#ifndef PINBRIDGE_RTN_VARARGS_BACKEND_H
#define PINBRIDGE_RTN_VARARGS_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendRtnInsertCall(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments);
PbStatus PbBackendRtnInsertCallProbed(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments, uint8_t* out_inserted);
PbStatus PbBackendRtnInsertCallProbedEx(
    PbRtnHandle routine, PbIpoint point, PbProbeMode mode,
    uint64_t callback_address, PbIargListHandle arguments,
    uint8_t* out_inserted);
PbStatus PbBackendRtnReplaceSignature(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);
PbStatus PbBackendRtnReplaceSignatureProbed(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);
PbStatus PbBackendRtnReplaceSignatureProbedEx(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);

#endif
