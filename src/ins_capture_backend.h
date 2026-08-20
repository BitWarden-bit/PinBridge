#ifndef PINBRIDGE_INS_CAPTURE_BACKEND_H
#define PINBRIDGE_INS_CAPTURE_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendInsInsertCaptureRegs(
    PbInsHandle ins, PbInsCaptureRegsCallback callback, void* user_data);
PbStatus PbBackendInsInsertHookMonitor(
    PbInsHandle ins, PbInsHookMonitorCallback callback, void* user_data);
PbStatus PbBackendInsInsertCaptureRegsCtx(
    PbInsHandle ins, PbInsContextCaptureRegsCallback callback, void* user_data);
PbStatus PbBackendInsInsertMemoryOperands(
    PbInsHandle ins, PbInsMemoryOperandCallback callback, void* user_data);
PbStatus PbBackendInsInsertMemoryAddressTranslation(
    PbInsHandle ins, PbInsMemoryTranslateCallback callback, void* user_data,
    PbRegId scratch_reg0, PbRegId scratch_reg1);
PbStatus PbBackendInsInsertExec(
    PbInsHandle ins, PbInsExecCallback callback, void* user_data);
PbStatus PbBackendInsInsertBranchEdge(
    PbInsHandle ins, PbInsBranchEdgeCallback callback, void* user_data);
PbStatus PbBackendInsInsertCaptureExecBytes(
    PbInsHandle ins, PbInsExecBytesCallback callback, void* user_data);
PbStatus PbBackendInsInsertMemoryOperandsValues(
    PbInsHandle ins, PbInsMemoryOperandValueCallback callback, void* user_data);

#endif
