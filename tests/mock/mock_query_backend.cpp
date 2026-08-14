#include "pin_query_backend.h"

uint64_t PbBackendInsQuery(uint32_t query_id, int32_t ins, uint64_t argument)
{
    if (query_id == PB_INS_QUERY_ID_pb_ins_address)
        return ins == 42 ? UINT64_C(0x401000) : 0;
    if (query_id == PB_INS_QUERY_ID_pb_ins_size)
        return ins == 42 ? 7u : 0u;
    return (static_cast<uint64_t>(query_id) << 32u) ^ argument ^ static_cast<uint32_t>(ins);
}
