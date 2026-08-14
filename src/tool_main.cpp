#include "pinbridge/pinbridge.h"

/* Minimal PinTool entry point. Pin calls main() in the DLL named by -t.
 * This default tool performs no instrumentation: it initializes Pin through
 * the C ABI and starts the application with Pin's default configuration.
 * A consumer DLL (C, Rust, ...) that wants to own main() links this library
 * and provides its own entry instead of this translation unit. */
int main(int argc, char* argv[])
{
    if (pb_pin_init(argc, argv) != PB_OK)
        return 1;
    pb_pin_start_program_default(); /* never returns */
    return 0;
}
