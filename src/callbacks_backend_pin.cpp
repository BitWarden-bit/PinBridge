#include "pin.H"

#include "callbacks_backend.h"

namespace
{

static_assert(sizeof(PIN_CALLBACK) <= sizeof(uint64_t),
              "PIN_CALLBACK no longer fits PbCallbackHandle");
static_assert(CALL_ORDER_FIRST == 100, "Pin CALL_ORDER_FIRST changed");
static_assert(CALL_ORDER_DEFAULT == 200, "Pin CALL_ORDER_DEFAULT changed");
static_assert(CALL_ORDER_LAST == 300, "Pin CALL_ORDER_LAST changed");

PIN_CALLBACK ToPinCallback(PbCallbackHandle callback)
{
    return reinterpret_cast<PIN_CALLBACK>(
        static_cast<uintptr_t>(callback.opaque));
}

} // namespace

PbStatus PbBackendCallbackGetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder* out_order)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    *out_order = static_cast<PbCallOrder>(
        CALLBACK_GetExecutionOrder(ToPinCallback(callback)));
    return PB_OK;
}

PbStatus PbBackendCallbackSetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder order)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    CALLBACK_SetExecutionOrder(
        ToPinCallback(callback), static_cast<CALL_ORDER>(order));
    return PB_OK;
}
