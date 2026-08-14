#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    char source[] = "safe-copy-ex";
    char destination[sizeof(source)] = {0};
    PbExceptionInfoSnapshot exception_info;
    uint64_t copied = 0;

    memset(&exception_info, 0xff, sizeof(exception_info));
    if (pb_pin_safe_copy_ex(
            destination, (uint64_t)(uintptr_t)source, sizeof(source),
            &copied, &exception_info) != PB_OK)
        return 1;
    if (copied != sizeof(source) || memcmp(source, destination, sizeof(source)) != 0 ||
        exception_info.flags != 0u)
        return 2;

    memset(&exception_info, 0xff, sizeof(exception_info));
    if (pb_pin_safe_copy_ex(destination, 0, 1, &copied, &exception_info) != PB_OK)
        return 3;
    if (copied != 0 || exception_info.exception_code != 1u ||
        exception_info.exception_class != 1u ||
        exception_info.exception_address != 0 ||
        exception_info.flags != PB_EXCEPTION_INFO_HAS_FAULT_ADDRESS ||
        exception_info.faulty_access_type != 1u ||
        exception_info.faulty_access_address != 0 ||
        exception_info.fp_errors != 0 || exception_info.windows_exception_code != 0 ||
        exception_info.windows_argument_count != 0)
        return 4;

    if (pb_pin_safe_copy_ex(0, 0, 1, &copied, &exception_info) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_safe_copy_ex(destination, 0, 1, 0, &exception_info) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_pin_safe_copy_ex(destination, 0, 1, &copied, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
