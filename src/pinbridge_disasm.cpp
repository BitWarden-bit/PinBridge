#include "pinbridge/pinbridge.h"

#include "disasm_backend.h"

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

} // namespace

PbStatus PB_CALL pb_disassemble(
    const uint8_t* bytes, uint64_t size, uint64_t address,
    PbDisasmInsn* out, uint64_t capacity, uint64_t* out_count)
{
    if (!bytes || size == 0 || !out || capacity == 0 || !out_count)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        *out_count = 0;
        return PbBackendDisassemble(bytes, size, address, out, capacity, out_count);
    });
}

PbStatus PB_CALL pb_disassemble_flow(
    const uint8_t* bytes, uint64_t size, uint64_t address, PbFlowInsn* out)
{
    if (!bytes || size == 0 || !out)
        return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        out->size = 0;
        return PbBackendDisassembleFlow(bytes, size, address, out);
    });
}
