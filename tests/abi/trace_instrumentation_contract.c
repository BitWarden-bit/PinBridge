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
    PbTraceHandle trace = (PbTraceHandle)(uintptr_t)7;

    if (pb_trace_insert_call_before(trace, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_analysis_calls != 1)
        return 1;
    g_predicate_value = 1;
    if (pb_trace_insert_if_call_before(trace, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_trace_insert_then_call_before(trace, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_predicate_calls != 1 || g_analysis_calls != 2)
        return 2;
    g_predicate_value = 0;
    if (pb_trace_insert_if_call_before(trace, OnPredicate, &g_predicate_calls) != PB_OK ||
        pb_trace_insert_then_call_before(trace, OnAnalysis, &g_analysis_calls) != PB_OK ||
        g_predicate_calls != 2 || g_analysis_calls != 2)
        return 3;

    if (pb_trace_insert_call_before(0, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_insert_if_call_before(0, OnPredicate, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_insert_then_call_before(0, OnAnalysis, 0) != PB_ERR_INVALID_ARGUMENT)
        return 4;
    if (pb_trace_insert_call_before(trace, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_insert_if_call_before(trace, 0, 0) != PB_ERR_INVALID_ARGUMENT ||
        pb_trace_insert_then_call_before(trace, 0, 0) != PB_ERR_INVALID_ARGUMENT)
        return 5;
    return 0;
}
