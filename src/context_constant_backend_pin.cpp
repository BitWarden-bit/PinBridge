#include "pin.H"

#include "context_constant_backend.h"

uint64_t PbBackendContextConstant(uint32_t constant_id)
{
    switch (constant_id)
    {
#if defined(TARGET_IA32)
    // Pin's ia32 register header only exposes the AMX constants under
    // TARGET_IA32E. Keep the generated ABI table (and its stable IDs) intact,
    // but report the unavailable tile state values as zero on ia32.
#define NUM_TILE_AND_CFG_REGS 0
#define NUM_TILE_REGS 0
#define TILECFG_SIZE_BYTES 0
#define TILE_SIZE_BYTES 0
#define TILE_STATE_SIZE 0
#endif
#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    case PB_CONTEXT_CONSTANT_ID_##pin_symbol: return static_cast<uint64_t>(pin_symbol);
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT
#if defined(TARGET_IA32)
#undef NUM_TILE_AND_CFG_REGS
#undef NUM_TILE_REGS
#undef TILECFG_SIZE_BYTES
#undef TILE_SIZE_BYTES
#undef TILE_STATE_SIZE
#endif
    default: return 0;
    }
}
