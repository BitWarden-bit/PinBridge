#include "pinbridge/pinbridge.h"

#include "ins_instrumentation_backend.h"

namespace
{

template< typename Function > PbStatus Guard(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsValidIpoint(PbIpoint ipoint)
{
    return ipoint == PB_IPOINT_BEFORE || ipoint == PB_IPOINT_AFTER;
}

template< typename Callback >
PbStatus Validate(PbInsHandle ins, Callback callback)
{
    return ins.opaque > 0 && callback ? PB_OK : PB_ERR_INVALID_ARGUMENT;
}

PbStatus ValidateFill(PbInsHandle ins, PbIpoint ipoint, PbBufferId id)
{
    return ins.opaque > 0 && IsValidIpoint(ipoint) && id != PB_BUFFER_ID_INVALID
        ? PB_OK : PB_ERR_INVALID_ARGUMENT;
}

} // namespace

#define PB_INS_CALL_WRAPPER(name, callback_type, backend) \
PbStatus PB_CALL name(PbInsHandle ins, callback_type callback, void* user_data) \
{ \
    const PbStatus valid = Validate(ins, callback); \
    if (valid != PB_OK) return valid; \
    return Guard([&]() { return backend(ins, callback, user_data); }); \
}

PB_INS_CALL_WRAPPER(pb_ins_insert_call_before, PbInsAnalysisCallback,
                    PbBackendInsInsertCallBefore)
PB_INS_CALL_WRAPPER(pb_ins_insert_call_before_ctx, PbInsContextAnalysisCallback,
                    PbBackendInsInsertCallBeforeCtx)
PB_INS_CALL_WRAPPER(pb_ins_insert_if_call_before, PbInsPredicateCallback,
                    PbBackendInsInsertIfCallBefore)
PB_INS_CALL_WRAPPER(pb_ins_insert_then_call_before, PbInsAnalysisCallback,
                    PbBackendInsInsertThenCallBefore)
PB_INS_CALL_WRAPPER(pb_ins_insert_predicated_call_before, PbInsAnalysisCallback,
                    PbBackendInsInsertPredicatedCallBefore)
PB_INS_CALL_WRAPPER(pb_ins_insert_if_predicated_call_before, PbInsPredicateCallback,
                    PbBackendInsInsertIfPredicatedCallBefore)
PB_INS_CALL_WRAPPER(pb_ins_insert_then_predicated_call_before, PbInsAnalysisCallback,
                    PbBackendInsInsertThenPredicatedCallBefore)

#undef PB_INS_CALL_WRAPPER

#define PB_INS_FILL_WRAPPER(name, backend) \
PbStatus PB_CALL name(PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t offset) \
{ \
    const PbStatus valid = ValidateFill(ins, ipoint, id); \
    if (valid != PB_OK) return valid; \
    return Guard([&]() { return backend(ins, ipoint, id, offset); }); \
}

PB_INS_FILL_WRAPPER(pb_ins_insert_fill_buffer, PbBackendInsInsertFillBuffer)
PB_INS_FILL_WRAPPER(pb_ins_insert_fill_buffer_predicated,
                    PbBackendInsInsertFillBufferPredicated)
PB_INS_FILL_WRAPPER(pb_ins_insert_fill_buffer_then,
                    PbBackendInsInsertFillBufferThen)

#undef PB_INS_FILL_WRAPPER
