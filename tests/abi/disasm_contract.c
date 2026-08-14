/* Contract test for pb_disassemble (ABI v1.2). */

#include "pinbridge/pinbridge.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond) \
    do { \
        if (!(cond)) { \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
            ++failures; \
        } \
    } while (0)

int main(void)
{
    static const unsigned char code[] = { 0x90, 0x90, 0xc3 };
    PbDisasmInsn insns[4];
    uint64_t count = 0;
    PbStatus status;

    memset(insns, 0, sizeof(insns));

    status = pb_disassemble(0, 3, 0x1000, insns, 4, &count);
    CHECK(status == PB_ERR_INVALID_ARGUMENT);
    status = pb_disassemble(code, 0, 0x1000, insns, 4, &count);
    CHECK(status == PB_ERR_INVALID_ARGUMENT);
    status = pb_disassemble(code, 3, 0x1000, 0, 4, &count);
    CHECK(status == PB_ERR_INVALID_ARGUMENT);
    status = pb_disassemble(code, 3, 0x1000, insns, 0, &count);
    CHECK(status == PB_ERR_INVALID_ARGUMENT);
    status = pb_disassemble(code, 3, 0x1000, insns, 4, 0);
    CHECK(status == PB_ERR_INVALID_ARGUMENT);

    status = pb_disassemble(code, 3, 0x1000, insns, 4, &count);
    CHECK(status == PB_OK);
    CHECK(count == 1);            /* mock emits exactly one nop */
    CHECK(insns[0].address == 0x1000);
    CHECK(insns[0].size == 1);
    CHECK(strcmp(insns[0].text, "nop") == 0);

    if (failures != 0)
    {
        printf("disasm_contract: %d failures\n", failures);
        return 1;
    }
    printf("disasm_contract: PASS\n");
    return 0;
}
