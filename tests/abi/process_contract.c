#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    uint8_t tile_config[64] = {0};
    uint8_t palette = 0;
    uint8_t active = 0;
    uint32_t value = 0;
    int32_t pid = 0;
    void (PB_CALL* exit_application)(int32_t) = pb_pin_exit_application;
    void (PB_CALL* exit_process)(int32_t) = pb_pin_exit_process;

    tile_config[0] = 1u;
    tile_config[16] = 0x34u;
    tile_config[17] = 0x12u;
    tile_config[48] = 7u;

    if (!exit_application || !exit_process)
        return 1;
    if (pb_pin_get_pid(&pid) != PB_OK || pid != 0x5031)
        return 2;
    if (pb_pin_is_amx_active(7u, &active) != PB_OK || active != 1u)
        return 3;
    if (pb_tile_config_get_palette_id(
            tile_config, sizeof(tile_config), &palette) != PB_OK || palette != 1u)
        return 4;
    if (pb_tile_config_get_tile_bytes_per_row(
            tile_config, sizeof(tile_config), PB_REG_TMM0, &value) != PB_OK ||
        value != UINT32_C(0x1234))
        return 5;
    if (pb_tile_config_get_tile_rows(
            tile_config, sizeof(tile_config), PB_REG_TMM0, &value) != PB_OK ||
        value != 7u)
        return 6;

    if (pb_pin_get_pid(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_is_amx_active(7u, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_tile_config_get_palette_id(0, sizeof(tile_config), &palette) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_tile_config_get_palette_id(tile_config, 63u, &palette) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_tile_config_get_palette_id(tile_config, sizeof(tile_config), 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_tile_config_get_tile_rows(
            tile_config, sizeof(tile_config), PB_REG_TMM0 - 1u, &value) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_tile_config_get_tile_bytes_per_row(
            tile_config, sizeof(tile_config), PB_REG_TMM7 + 1u, &value) !=
            PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
