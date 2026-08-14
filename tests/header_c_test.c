/* Host-side C11 compile check for the frozen public ABI header.
 * The header must be self-contained, C11-clean, and free of Pin types. */

#include "pinbridge/pinbridge.h"

/* ABI v1.3 identity. */
_Static_assert(PB_ABI_VERSION_MAJOR == 1u, "ABI major must stay 1 in this snapshot");
_Static_assert(PB_ABI_VERSION_MINOR == 5u, "ABI minor tracks v1.1..v1.5 additions");

/* Status codes are part of the contract. */
_Static_assert(PB_OK == 0, "PB_OK must be 0");
_Static_assert(PB_ERR_INTERNAL == 7, "PbStatus range drifted");

/* Fixed handle and snapshot layouts. */
_Static_assert(sizeof(PbInsHandle) == 4, "PbInsHandle must stay a 32-bit token");
_Static_assert(sizeof(PbCallbackHandle) == 8, "PbCallbackHandle must stay 64-bit");
_Static_assert(sizeof(PbFxSave) == 512, "PbFxSave must stay 512 bytes");
_Static_assert(sizeof(void*) == 8, "Windows x64 only");

int main(void)
{
    return (int)(pb_abi_version() == PB_ABI_VERSION ? 0 : 1);
}
