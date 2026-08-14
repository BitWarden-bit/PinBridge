#ifndef PINBRIDGE_CONTROL_QUERY_BACKEND_H
#define PINBRIDGE_CONTROL_QUERY_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendCheckReadAccess(uint64_t address, uint8_t* out_accessible);
PbStatus PbBackendCheckWriteAccess(uint64_t address, uint8_t* out_accessible);
PbStatus PbBackendIsAttaching(uint8_t* out_attaching);
PbStatus PbBackendIsProbeMode(uint8_t* out_probe_mode);
PbStatus PbBackendIsSafeForProbedInsertion(uint64_t address, uint8_t* out_safe);
const char* PbBackendToolFullPath(void);

#endif
