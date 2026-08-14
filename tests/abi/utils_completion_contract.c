#include <stdint.h>
#include <string.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    const char input[] = "\n# ignored\nalpha\nbeta\n";
    char line[32] = {0};
    uint64_t required = 0;
    uint64_t next_offset = 0;
    uint32_t next_line = 0;
    uint8_t storage[16] = {0};
    void* pointer = 0;
    const void* const_pointer = 0;

    if (pb_ptr_at_offset_typed(storage, 3, &pointer) != PB_OK ||
        pointer != storage + 3 ||
        pb_const_ptr_at_offset_typed(storage, 5, &const_pointer) != PB_OK ||
        const_pointer != storage + 5)
        return 5;

    if (pb_read_line(input, sizeof(input) - 1, 0, 0, 0, 0, &required,
            &next_offset, &next_line) != PB_ERR_BUFFER_TOO_SMALL ||
        required == 0 || required > sizeof(line) || next_offset == 0 ||
        next_line == 0)
        return 1;
    if (pb_read_line(input, sizeof(input) - 1, 0, 0, line, sizeof(line),
            &required, &next_offset, &next_line) != PB_OK ||
        strcmp(line, "alpha") != 0)
        return 2;
    if (pb_read_line(input, sizeof(input) - 1, next_offset, next_line,
            line, sizeof(line), &required, &next_offset, &next_line) != PB_OK ||
        strcmp(line, "beta") != 0)
        return 3;
    if (pb_read_line(0, 0, 0, 0, line, sizeof(line), &required,
            &next_offset, &next_line) != PB_ERR_INVALID_ARGUMENT ||
        pb_read_line(input, sizeof(input), sizeof(input) + 1, 0,
            line, sizeof(line), &required, &next_offset, &next_line) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_read_line(input, sizeof(input), 0, 0, line, sizeof(line), 0,
            &next_offset, &next_line) != PB_ERR_INVALID_ARGUMENT ||
        pb_read_line(input, sizeof(input), 0, 0, line, sizeof(line), &required,
            0, &next_line) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
