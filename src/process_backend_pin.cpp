#include "pin.H"

#include "process_backend.h"

namespace
{

bool InProbeMode()
{
    return PIN_IsProbeMode() != 0;
}

#if defined(TARGET_IA32E)
bool ValidTileConfigWidth()
{
    return REG_Size(REG_TILECONFIG) == 64;
}
#endif

} // namespace

PB_NORETURN void PbBackendExitApplication(int32_t status)
{
    PIN_ExitApplication(static_cast<INT32>(status));
}

PB_NORETURN void PbBackendExitProcess(int32_t exit_code)
{
    PIN_ExitProcess(static_cast<INT32>(exit_code));
}

PbStatus PbBackendGetPid(int32_t* out_pid)
{
    *out_pid = static_cast<int32_t>(PIN_GetPid());
    return PB_OK;
}

PbStatus PbBackendIsAmxActive(PbThreadId thread_id, uint8_t* out_active)
{
#if defined(TARGET_IA32E)
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_active = PIN_IsAmxActive(static_cast<THREADID>(thread_id)) ? 1u : 0u;
    return PB_OK;
#else
    // AMX tile state does not exist on ia32; there is no register to query.
    (void)thread_id;
    (void)out_active;
    return PB_ERR_UNSUPPORTED;
#endif
}

PbStatus PbBackendTileConfigGetPaletteId(
    const uint8_t* tile_config, uint8_t* out_palette_id)
{
#if defined(TARGET_IA32E)
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    if (!ValidTileConfigWidth())
        return PB_ERR_INTERNAL;
    *out_palette_id = TileCfg_GetPaletteID(const_cast<UINT8*>(tile_config));
    return PB_OK;
#else
    (void)tile_config;
    (void)out_palette_id;
    return PB_ERR_UNSUPPORTED;
#endif
}

PbStatus PbBackendTileConfigGetTileBytesPerRow(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_bytes_per_row)
{
#if defined(TARGET_IA32E)
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    if (!ValidTileConfigWidth())
        return PB_ERR_INTERNAL;
    *out_bytes_per_row = TileCfg_GetTileBytesPerRow(
        const_cast<UINT8*>(tile_config), static_cast<REG>(tmm));
    return PB_OK;
#else
    (void)tile_config;
    (void)tmm;
    (void)out_bytes_per_row;
    return PB_ERR_UNSUPPORTED;
#endif
}

PbStatus PbBackendTileConfigGetTileRows(
    const uint8_t* tile_config, PbRegId tmm, uint32_t* out_rows)
{
#if defined(TARGET_IA32E)
    if (InProbeMode())
        return PB_ERR_INVALID_STATE;
    if (!ValidTileConfigWidth())
        return PB_ERR_INTERNAL;
    *out_rows = TileCfg_GetTileRows(
        const_cast<UINT8*>(tile_config), static_cast<REG>(tmm));
    return PB_OK;
#else
    (void)tile_config;
    (void)tmm;
    (void)out_rows;
    return PB_ERR_UNSUPPORTED;
#endif
}
