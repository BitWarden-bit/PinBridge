#include <stdint.h>

#include "pinbridge/pinbridge.h"

static uint32_t g_analysis_calls;
static uint32_t g_predicate_calls;
static uint8_t g_predicate_value;

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
    PbBblHandle bbl = {7};

    if (pb_bbl_insert_call_before(bbl, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_analysis_calls != 1)
        return 1;
    g_predicate_value = 1;
    if (pb_bbl_insert_if_call_before(bbl, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_bbl_insert_then_call_before(bbl, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_predicate_calls != 1 || g_analysis_calls != 2)
        return 2;
    g_predicate_value = 0;
    if (pb_bbl_insert_if_call_before(bbl, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_bbl_insert_then_call_before(bbl, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_predicate_calls != 2 || g_analysis_calls != 2)
        return 3;

    bbl.opaque = 0;
    if (pb_bbl_insert_call_before(bbl, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_bbl_insert_if_call_before(bbl, OnPredicate, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_bbl_insert_then_call_before(bbl, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    bbl.opaque = 1;
    if (pb_bbl_insert_call_before(bbl, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_bbl_insert_if_call_before(bbl, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_bbl_insert_then_call_before(bbl, 0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
