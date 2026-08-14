#include "proto_backend.h"

namespace
{

struct MockProto
{
    uint32_t marker;
};

MockProto g_proto = {0};

} // namespace

PbStatus PbBackendProtoAllocate(
    PbProtoArg return_arg, PbCallingStandard calling_standard,
    const char* name, const PbProtoArg* descriptors,
    uint32_t descriptor_count, PbProtoHandle* out_proto)
{
    if (return_arg.kind != PB_PARG_UINT || return_arg.size != 4 ||
        calling_standard != PB_CALLINGSTD_DEFAULT || !name ||
        descriptor_count != 3 || descriptors[0].kind != PB_PARG_UINT ||
        descriptors[1].kind != PB_PARG_ENUM ||
        descriptors[2].kind != PB_PARG_END)
        return PB_ERR_INTERNAL;
    g_proto.marker = 0x50524f54u;
    *out_proto = reinterpret_cast<PbProtoHandle>(&g_proto);
    return PB_OK;
}

PbStatus PbBackendProtoFree(PbProtoHandle proto)
{
    if (proto != reinterpret_cast<PbProtoHandle>(&g_proto) ||
        g_proto.marker != 0x50524f54u)
        return PB_ERR_INTERNAL;
    g_proto.marker = 0;
    return PB_OK;
}
