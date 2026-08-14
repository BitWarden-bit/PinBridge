#include "pinbridge/pinbridge.h"

#include "control_state_backend.h"

#include <cstdlib>

namespace
{

template< typename Function > PbStatus GuardStatus(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

} // namespace

PbStatus PB_CALL pb_pin_init_symbols(void)
{
    return GuardStatus([]() -> PbStatus {
        PbBackendInitSymbols();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_init_symbols_alt(PbSymbolInfoMode mode, uint8_t* out_success)
{
    if (!out_success)
        return PB_ERR_INVALID_ARGUMENT;
    *out_success = 0;
    if ((mode & ~static_cast<PbSymbolInfoMode>(PB_DEBUG_OR_EXPORT_SYMBOLS)) != 0)
        return (mode & PB_IFUNC_SYMBOLS) != 0 ? PB_ERR_UNSUPPORTED : PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        *out_success = PbBackendInitSymbolsAlt(mode) ? 1u : 0u;
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_lock_client(void)
{
    return GuardStatus([]() -> PbStatus {
        PbBackendLockClient();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_unlock_client(void)
{
    return GuardStatus([]() -> PbStatus {
        PbBackendUnlockClient();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_set_smc_support(PbSmcMode mode)
{
    if (mode != PB_SMC_ENABLE && mode != PB_SMC_DISABLE)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        PbBackendSetSmcSupport(mode);
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_create_default_configuration_info(
    PbPinConfigurationHandle* out_configuration)
{
    if (!out_configuration)
        return PB_ERR_INVALID_ARGUMENT;
    *out_configuration = 0;
    return GuardStatus([&]() -> PbStatus {
        void* configuration = PbBackendCreateDefaultConfigurationInfo();
        if (!configuration)
            return PB_ERR_INTERNAL;
        *out_configuration = reinterpret_cast<PbPinConfigurationHandle>(configuration);
        return PB_OK;
    });
}

void PB_CALL pb_pin_start_program_configured(PbPinConfigurationHandle configuration)
{
    if (!configuration)
        std::abort();
    PbBackendStartProgramConfigured(reinterpret_cast<void*>(configuration));
    std::abort();
}

void PB_CALL pb_pin_start_program_probed(void)
{
    PbBackendStartProgramProbed();
    std::abort();
}

PbStatus PB_CALL pb_pin_remove_fini_functions(void)
{
    return GuardStatus([]() -> PbStatus {
        PbBackendRemoveFiniFunctions();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_remove_instrumentation(void)
{
    return GuardStatus([]() -> PbStatus {
        PbBackendRemoveInstrumentation();
        return PB_OK;
    });
}

PbStatus PB_CALL pb_pin_remove_instrumentation_in_range(uint64_t start, uint64_t end)
{
    if (start >= end)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardStatus([&]() -> PbStatus {
        PbBackendRemoveInstrumentationInRange(start, end);
        return PB_OK;
    });
}
