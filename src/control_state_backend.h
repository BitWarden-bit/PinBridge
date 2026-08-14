#ifndef PINBRIDGE_CONTROL_STATE_BACKEND_H
#define PINBRIDGE_CONTROL_STATE_BACKEND_H

#include <stdint.h>

void PbBackendInitSymbols(void);
uint8_t PbBackendInitSymbolsAlt(uint32_t mode);
void PbBackendLockClient(void);
void PbBackendUnlockClient(void);
void PbBackendSetSmcSupport(uint32_t mode);
void* PbBackendCreateDefaultConfigurationInfo(void);
void PbBackendStartProgramConfigured(void* configuration);
void PbBackendStartProgramProbed(void);
void PbBackendRemoveFiniFunctions(void);
void PbBackendRemoveInstrumentation(void);
void PbBackendRemoveInstrumentationInRange(uint64_t start, uint64_t end);

#endif
