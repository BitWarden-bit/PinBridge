#include "pinbridge/pinbridge.h"

#include "process_backend.h"

namespace
{

const uint64_t kTileConfigSize = 64;

bool IsTmm(PbRegId reg)
{
    return reg >= PB_REG_TMM0 && reg <= PB_REG_TMM7;
}

template< typename Function > PbStatus InvokeProcess(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}

} // namespace

PB_NORETURN void PB_CALL pb_pin_exit_application(int32_t status)
{
    PbBackendExitApplication(status);
}

PB_NORETURN void PB_CALL pb_pin_exit_process(int32_t exit_code)
{
    PbBackendExitProcess(exit_code);
}

PbStatus PB_CALL pb_pin_get_pid(int32_t* out_pid)
{
    if (!out_pid)
        return PB_ERR_INVALID_ARGUMENT;
    *out_pid = 0;
    return InvokeProcess([&]() { return PbBackendGetPid(out_pid); });
}

PbStatus PB_CALL pb_pin_is_amx_active(
    PbThreadId thread_id, uint8_t* out_active)
{
    if (!out_active)
        return PB_ERR_INVALID_ARGUMENT;
    *out_active = 0;
    return InvokeProcess(
        [&]() { return PbBackendIsAmxActive(thread_id, out_active); });
}

PbStatus PB_CALL pb_tile_config_get_palette_id(
    const uint8_t* tile_config, uint64_t tile_config_size, uint8_t* out_palette_id)
{
    if (!tile_config || tile_config_size < kTileConfigSize || !out_palette_id)
        return PB_ERR_INVALID_ARGUMENT;
    *out_palette_id = 0;
    return InvokeProcess([&]() {
        return PbBackendTileConfigGetPaletteId(tile_config, out_palette_id);
    });
}

PbStatus PB_CALL pb_tile_config_get_tile_bytes_per_row(
    const uint8_t* tile_config, uint64_t tile_config_size,
    PbRegId tmm, uint32_t* out_bytes_per_row)
{
    if (!tile_config || tile_config_size < kTileConfigSize ||
        !IsTmm(tmm) || !out_bytes_per_row)
        return PB_ERR_INVALID_ARGUMENT;
    *out_bytes_per_row = 0;
    return InvokeProcess([&]() {
        return PbBackendTileConfigGetTileBytesPerRow(
            tile_config, tmm, out_bytes_per_row);
    });
}

PbStatus PB_CALL pb_tile_config_get_tile_rows(
    const uint8_t* tile_config, uint64_t tile_config_size,
    PbRegId tmm, uint32_t* out_rows)
{
    if (!tile_config || tile_config_size < kTileConfigSize ||
        !IsTmm(tmm) || !out_rows)
        return PB_ERR_INVALID_ARGUMENT;
    *out_rows = 0;
    return InvokeProcess(
        [&]() { return PbBackendTileConfigGetTileRows(tile_config, tmm, out_rows); });
}
