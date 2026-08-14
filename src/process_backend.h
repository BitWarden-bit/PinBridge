#ifndef PINBRIDGE_PROCESS_BACKEND_H
#define PINBRIDGE_PROCESS_BACKEND_H

#include "pinbridge/pinbridge.h"

PB_NORETURN void PbBackendExitApplication(int32_t status);
PB_NORETURN void PbBackendExitProcess(int32_t exit_code);
PbStatus PbBackendGetPid(int32_t* out_pid);
PbStatus PbBackendIsAmxActive(PbThreadId thread_id, uint8_t* out_active);
PbStatus PbBackendTileConfigGetPaletteId(
    const uint8_t* tile_config, uint8_t* out_palette_id);
PbStatus PbBackendTileConfigGetTileBytesPerRow(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_bytes_per_row);
PbStatus PbBackendTileConfigGetTileRows(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_rows);

#endif
