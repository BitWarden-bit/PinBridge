#include "callbacks_backend.h"

namespace
{

PbCallOrder g_order = PB_CALL_ORDER_DEFAULT;

bool IsExpectedCallback(PbCallbackHandle callback)
{
    return callback.opaque == UINT64_C(0x4801);
}

} // namespace

PbStatus PbBackendCallbackGetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder* out_order)
{
    if (!IsExpectedCallback(callback))
        return PB_ERR_INTERNAL;
    *out_order = g_order;
    return PB_OK;
}

PbStatus PbBackendCallbackSetExecutionOrder(
    PbCallbackHandle callback, PbCallOrder order)
{
    if (!IsExpectedCallback(callback))
        return PB_ERR_INTERNAL;
    g_order = order;
    return PB_OK;
}
