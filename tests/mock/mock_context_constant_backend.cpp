#include "context_constant_backend.h"

uint64_t PbBackendContextConstant(uint32_t constant_id)
{
    return static_cast<uint64_t>(constant_id) + 1u;
}
