#include "inst_args_backend.h"

namespace
{

struct MockIargList
{
    uint32_t marker;
    uint32_t count;
};

MockIargList g_list = {0, 0};

} // namespace

PbStatus PbBackendIargListAlloc(PbIargListHandle* out_list)
{
    g_list.marker = 0x49415247u;
    g_list.count = 0;
    *out_list = reinterpret_cast<PbIargListHandle>(&g_list);
    return PB_OK;
}

PbStatus PbBackendIargListAdd(
    PbIargListHandle list, const PbIargDescriptor*, uint32_t descriptor_count)
{
    if (list != reinterpret_cast<PbIargListHandle>(&g_list) ||
        g_list.marker != 0x49415247u)
        return PB_ERR_INTERNAL;
    g_list.count += descriptor_count;
    return PB_OK;
}

PbStatus PbBackendIargListFree(PbIargListHandle list)
{
    if (list != reinterpret_cast<PbIargListHandle>(&g_list) ||
        g_list.marker != 0x49415247u || g_list.count != 4)
        return PB_ERR_INTERNAL;
    g_list.marker = 0;
    g_list.count = 0;
    return PB_OK;
}

void* PbBackendIargListNative(PbIargListHandle list)
{
    return list;
}
