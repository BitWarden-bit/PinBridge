#include "control_attach_backend.h"

PbStatus PbBackendAttach(
    PbAttachCallback callback, void* user_data, PbAttachStatus* out_status)
{
    *out_status = PB_ATTACH_INITIATED;
    callback(user_data);
    return PB_OK;
}

PbStatus PbBackendAttachProbed(
    PbAttachProbedCallback callback, void* user_data, PbAttachStatus* out_status)
{
    *out_status = PB_ATTACH_INITIATED;
    callback(user_data);
    return PB_OK;
}
