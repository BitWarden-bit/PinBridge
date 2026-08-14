#include "pin.H"

#include "control_state_backend.h"

void PbBackendInitSymbols(void) { PIN_InitSymbols(); }

uint8_t PbBackendInitSymbolsAlt(uint32_t mode)
{
    return PIN_InitSymbolsAlt(static_cast<SYMBOL_INFO_MODE>(mode)) ? 1u : 0u;
}

void PbBackendLockClient(void) { PIN_LockClient(); }

void PbBackendUnlockClient(void) { PIN_UnlockClient(); }

void PbBackendSetSmcSupport(uint32_t mode)
{
    PIN_SetSmcSupport(static_cast<SMC_ENABLE_DISABLE_TYPE>(mode));
}

void* PbBackendCreateDefaultConfigurationInfo(void)
{
    return PIN_CreateDefaultConfigurationInfo();
}

void PbBackendStartProgramConfigured(void* configuration)
{
    PIN_StartProgram(static_cast<PIN_CONFIGURATION_INFO>(configuration));
}

void PbBackendStartProgramProbed(void) { PIN_StartProgramProbed(); }

void PbBackendRemoveFiniFunctions(void) { PIN_RemoveFiniFunctions(); }

void PbBackendRemoveInstrumentation(void) { PIN_RemoveInstrumentation(); }

void PbBackendRemoveInstrumentationInRange(uint64_t start, uint64_t end)
{
    PIN_RemoveInstrumentationInRange(static_cast<ADDRINT>(start), static_cast<ADDRINT>(end));
}
