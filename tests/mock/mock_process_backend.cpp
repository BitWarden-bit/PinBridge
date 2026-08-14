#include "process_backend.h"

#include <cstdlib>

PB_NORETURN void PbBackendExitApplication(int32_t)
{
    std::abort();
}

PB_NORETURN void PbBackendExitProcess(int32_t)
{
    std::abort();
}

PbStatus PbBackendGetPid(int32_t* out_pid)
{
    *out_pid = 0x5031;
    return PB_OK;
}

PbStatus PbBackendIsAmxActive(PbThreadId thread_id, uint8_t* out_active)
{
    *out_active = thread_id == 7u ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendTileConfigGetPaletteId(
    const uint8_t* tile_config, uint8_t* out_palette_id)
{
    *out_palette_id = tile_config[0];
    return PB_OK;
}

PbStatus PbBackendTileConfigGetTileBytesPerRow(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_bytes_per_row)
{
    const uint32_t index = tmm - PB_REG_TMM0;
    *out_bytes_per_row =
        static_cast<uint32_t>(tile_config[16u + index * 2u]) |
        (static_cast<uint32_t>(tile_config[17u + index * 2u]) << 8u);
    return PB_OK;
}

PbStatus PbBackendTileConfigGetTileRows(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_rows)
{
    *out_rows = tile_config[48u + (tmm - PB_REG_TMM0)];
    return PB_OK;
}
