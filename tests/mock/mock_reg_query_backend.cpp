#include "reg_query_backend.h"

uint64_t PbBackendRegQuery(uint32_t query_id, uint64_t argument)
{
    return (static_cast<uint64_t>(query_id) << 32u) ^ argument ^ UINT64_C(0x524547);
}
