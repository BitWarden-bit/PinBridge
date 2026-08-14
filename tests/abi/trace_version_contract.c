#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbBblHandle bbl = {1};
    PbInsHandle ins = {2};
    PbTraceHandle trace = (PbTraceHandle)(uintptr_t)3;
    uint64_t version = 0;

    if ((PbCallOrder)PB_CALL_ORDER_FIRST != (PbCallOrder)100 ||
        (PbCallOrder)PB_CALL_ORDER_DEFAULT != (PbCallOrder)200 ||
        (PbCallOrder)PB_CALL_ORDER_LAST != (PbCallOrder)300)
        return 1;
    if (pb_bbl_set_target_version(bbl, UINT64_C(7)) != PB_OK)
        return 2;
    if (pb_trace_version(trace, &version) != PB_OK || version != UINT64_C(11))
        return 3;
    if (pb_ins_insert_version_case(
            ins, PB_REG_INST_G0, INT32_C(-5), UINT64_C(13)) != PB_OK)
        return 4;
    if (pb_ins_insert_version_case_with_call_order(
            ins, PB_REG_INST_G0, INT32_C(5), UINT64_C(17),
            (PbCallOrder)(PB_CALL_ORDER_FIRST + 5)) != PB_OK)
        return 5;

    bbl.opaque = 0;
    if (pb_bbl_set_target_version(bbl, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_version(0, &version) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_version(trace, 0) != PB_ERR_INVALID_ARGUMENT ||
        (ins.opaque = 0,
         pb_ins_insert_version_case(
            ins, PB_REG_INST_G0, 0, 0)) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_version_case_with_call_order(
            ins, PB_REG_INST_G0, 0, 0,
            PB_CALL_ORDER_DEFAULT) != PB_ERR_INVALID_ARGUMENT)
        return 6;
    return 0;
}
