#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbInsHandle ins = {2};
    if (PB_IPOINT_INVALID != 0 || PB_IPOINT_BEFORE != 1 ||
        PB_IPOINT_AFTER != 2 || PB_IPOINT_ANYWHERE != 3 ||
        PB_IPOINT_TAKEN_BRANCH != 4)
        return 1;
    if (pb_ins_delete(ins) != PB_OK ||
        pb_ins_insert_direct_jump(ins, PB_IPOINT_BEFORE, UINT64_C(0x1234)) != PB_OK ||
        pb_ins_insert_direct_jump(ins, PB_IPOINT_AFTER, UINT64_C(0x1234)) != PB_OK ||
        pb_ins_insert_indirect_jump(ins, PB_IPOINT_BEFORE, PB_REG_RAX) != PB_OK ||
        pb_ins_rewrite_memory_operand(ins, 0, PB_REG_RAX) != PB_OK ||
        pb_ins_rewrite_memory_operand(ins, 1, PB_REG_RAX) != PB_OK)
        return 2;
    PbInsHandle scattered = {3};
    if (pb_ins_rewrite_scattered_memory_operand(scattered, 0) != PB_OK)
        return 3;
    if (pb_ins_delete((PbInsHandle){0}) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_direct_jump(ins, PB_IPOINT_ANYWHERE, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_direct_jump(ins, PB_IPOINT_TAKEN_BRANCH, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_indirect_jump(ins, PB_IPOINT_BEFORE, PB_REG_INVALID_) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_rewrite_memory_operand(ins, UINT32_MAX, PB_REG_RAX) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_rewrite_memory_operand(ins, 0, PB_REG_INVALID_) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_rewrite_memory_operand(scattered, 0, PB_REG_RAX) != PB_ERR_UNSUPPORTED ||
        pb_ins_rewrite_scattered_memory_operand(ins, 0) != PB_ERR_UNSUPPORTED ||
        pb_ins_rewrite_scattered_memory_operand(scattered, 1) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    return 0;
}
