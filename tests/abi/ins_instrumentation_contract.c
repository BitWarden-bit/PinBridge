#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_analysis_calls;
static uint32_t g_predicate_calls;
static uint64_t g_predicate_value = 1;

static void PB_CALL OnAnalysis(void* user_data)
{
    if (user_data == &g_analysis_calls)
        ++g_analysis_calls;
}

static uint64_t PB_CALL OnPredicate(void* user_data)
{
    if (user_data == &g_predicate_calls)
        ++g_predicate_calls;
    return g_predicate_value;
}

int main(void)
{
    PbInsHandle ins = {7};

    if (pb_ins_insert_call_before(ins, OnAnalysis, &g_analysis_calls) != PB_OK ||
        pb_ins_insert_if_call_before(ins, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_ins_insert_then_call_before(ins, OnAnalysis, &g_analysis_calls) != PB_OK ||
        pb_ins_insert_predicated_call_before(ins, OnAnalysis, &g_analysis_calls) != PB_OK ||
        pb_ins_insert_if_predicated_call_before(ins, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_ins_insert_then_predicated_call_before(ins, OnAnalysis, &g_analysis_calls) != PB_OK)
        return 1;
    if (pb_ins_insert_fill_buffer(ins, PB_IPOINT_BEFORE, 7, 0) != PB_OK ||
        pb_ins_insert_fill_buffer_predicated(ins, PB_IPOINT_AFTER, 7, 8) != PB_OK ||
        pb_ins_insert_fill_buffer_then(ins, PB_IPOINT_BEFORE, 7, 16) != PB_OK)
        return 2;
    if (g_analysis_calls != 4 || g_predicate_calls != 2)
        return 3;
    g_predicate_value = 0;
    if (pb_ins_insert_if_call_before(
            ins, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_ins_insert_then_call_before(
            ins, OnAnalysis, &g_analysis_calls) != PB_OK ||
        pb_ins_insert_if_predicated_call_before(
            ins, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_ins_insert_then_predicated_call_before(
            ins, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_analysis_calls != 4 || g_predicate_calls != 4)
        return 4;

    ins.opaque = 0;
    if (pb_ins_insert_call_before(ins, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_if_call_before(ins, OnPredicate, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_then_call_before(ins, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_predicated_call_before(ins, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_if_predicated_call_before(ins, OnPredicate, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_then_predicated_call_before(ins, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_fill_buffer(ins, PB_IPOINT_BEFORE, 7, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    ins.opaque = 1;
    if (pb_ins_insert_call_before(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_if_call_before(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_then_call_before(ins, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_predicated_call_before(ins, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_if_predicated_call_before(ins, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_then_predicated_call_before(ins, 0, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 6;
    if (pb_ins_insert_fill_buffer(
            ins, PB_IPOINT_INVALID, 1, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_fill_buffer_predicated(
            ins, PB_IPOINT_ANYWHERE, 1, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_ins_insert_fill_buffer_then(
            ins, PB_IPOINT_BEFORE, PB_BUFFER_ID_INVALID, 0) !=
            PB_ERR_INVALID_ARGUMENT)
        return 7;
    return 0;
}
