#include "control_state_backend.h"

void PbBackendInitSymbols(void) {}

uint8_t PbBackendInitSymbolsAlt(uint32_t mode) { return mode <= 3u ? 1u : 0u; }

void PbBackendLockClient(void) {}

void PbBackendUnlockClient(void) {}

void PbBackendSetSmcSupport(uint32_t) {}

void* PbBackendCreateDefaultConfigurationInfo(void)
{
    return reinterpret_cast<void*>(static_cast<uintptr_t>(UINT64_C(0x6000)));
}

void PbBackendStartProgramConfigured(void*) {}

void PbBackendStartProgramProbed(void) {}

void PbBackendRemoveFiniFunctions(void) {}

void PbBackendRemoveInstrumentation(void) {}

void PbBackendRemoveInstrumentationInRange(uint64_t, uint64_t) {}
