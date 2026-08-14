#ifndef PINBRIDGE_INS_MODIFICATION_BACKEND_H
#define PINBRIDGE_INS_MODIFICATION_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendInsDelete(PbInsHandle ins);
PbStatus PbBackendInsInsertDirectJump(
    PbInsHandle ins, PbIpoint ipoint, uint64_t target);
PbStatus PbBackendInsInsertIndirectJump(
    PbInsHandle ins, PbIpoint ipoint, PbRegId reg);
PbStatus PbBackendInsRewriteMemoryOperand(
    PbInsHandle ins, uint32_t memindex, PbRegId reg);
PbStatus PbBackendInsRewriteScatteredMemoryOperand(
    PbInsHandle ins, uint32_t memindex);

#endif
