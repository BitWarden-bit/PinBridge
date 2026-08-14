#ifndef PINBRIDGE_INS_INSPECTION_EXTRAS_BACKEND_H
#define PINBRIDGE_INS_INSPECTION_EXTRAS_BACKEND_H

#include "pinbridge/pinbridge.h"

uint8_t PbBackendInsInspectionExtrasIsProbeMode(void);
uint64_t PbBackendCategoryStringShort(
    uint32_t value, char* buffer, uint64_t capacity);
uint64_t PbBackendExtensionStringShort(
    uint32_t value, char* buffer, uint64_t capacity);
uint64_t PbBackendOpcodeStringShort(
    uint32_t value, char* buffer, uint64_t capacity);
uint64_t PbBackendInsDisassemble(
    PbInsHandle ins, char* buffer, uint64_t capacity);
uint64_t PbBackendInsMnemonic(
    PbInsHandle ins, char* buffer, uint64_t capacity);
void PbBackendInsGetNumberAndSizeOfMemAccesses(
    PbInsHandle ins, int32_t* num_accesses, int32_t* access_size,
    int32_t* index_size);
uint8_t PbBackendInsChangeReg(
    PbInsHandle ins, PbRegId old_reg, PbRegId new_reg, uint8_t as_read);
void PbBackendInsGetFarPointer(
    PbInsHandle ins, uint16_t* segment_selector, uint32_t* displacement);
int32_t PbBackendInsInvalid(void);
PbXedDecodedInstHandle PbBackendInsXedDec(PbInsHandle ins);
PbXedRegId PbBackendInsXedExactMapFromPinReg(PbRegId pin_reg);
PbRegId PbBackendInsXedExactMapToPinReg(PbXedRegId xed_reg);
PbRegId PbBackendInsXedExactMapToPinRegLegacy(uint32_t xed_reg);
void PbBackendPinSetSyntaxAtt(void);
void PbBackendPinSetSyntaxIntel(void);
void PbBackendPinSetSyntaxXed(void);

#endif
