#include <stdint.h>

#include "pinbridge/pinbridge.h"


int main(void)
{
    uint8_t success = 0;
    PbPinConfigurationHandle configuration = 0;

    if (pb_pin_init_symbols() != PB_OK ||
        pb_pin_init_symbols_alt(PB_DEBUG_OR_EXPORT_SYMBOLS, &success) != PB_OK ||
        success != 1)
        return 1;
    if (pb_pin_init_symbols_alt(PB_NO_SYMBOLS, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_init_symbols_alt(PB_IFUNC_SYMBOLS, &success) != PB_ERR_UNSUPPORTED)
        return 2;
    if (pb_pin_lock_client() != PB_OK || pb_pin_unlock_client() != PB_OK)
        return 3;
    if (pb_pin_set_smc_support(PB_SMC_ENABLE) != PB_OK ||
        pb_pin_set_smc_support((PbSmcMode)99u) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    if (pb_pin_create_default_configuration_info(&configuration) != PB_OK ||
        configuration == 0 ||
        pb_pin_create_default_configuration_info(0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    if (pb_pin_remove_fini_functions() != PB_OK ||
        pb_pin_remove_instrumentation() != PB_OK ||
        pb_pin_remove_instrumentation_in_range(UINT64_C(0x1000), UINT64_C(0x2000)) != PB_OK)
        return 6;
    if (pb_pin_remove_instrumentation_in_range(UINT64_C(0x1000), UINT64_C(0x1000)) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_remove_instrumentation_in_range(UINT64_C(0x2000), UINT64_C(0x1000)) !=
            PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
