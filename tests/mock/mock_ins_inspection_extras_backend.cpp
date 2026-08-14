#include "ins_inspection_extras_backend.h"

#include <cstring>

namespace
{
uint64_t Copy(const char* value, char* buffer, uint64_t capacity)
{
    const uint64_t required = static_cast<uint64_t>(std::strlen(value)) + 1u;
    if (buffer && capacity >= required)
        std::memcpy(buffer, value, static_cast<size_t>(required));
    return required;
}
}

uint8_t PbBackendInsInspectionExtrasIsProbeMode(void) { return 0; }
uint64_t PbBackendCategoryStringShort(uint32_t, char* buffer, uint64_t capacity)
{ return Copy("mock_category", buffer, capacity); }
uint64_t PbBackendExtensionStringShort(uint32_t, char* buffer, uint64_t capacity)
{ return Copy("mock_extension", buffer, capacity); }
uint64_t PbBackendOpcodeStringShort(uint32_t, char* buffer, uint64_t capacity)
{ return Copy("mock_opcode", buffer, capacity); }
uint64_t PbBackendInsDisassemble(PbInsHandle, char* buffer, uint64_t capacity)
{ return Copy("mock_disassembly", buffer, capacity); }
uint64_t PbBackendInsMnemonic(PbInsHandle, char* buffer, uint64_t capacity)
{ return Copy("mock_mnemonic", buffer, capacity); }

void PbBackendInsGetNumberAndSizeOfMemAccesses(
    PbInsHandle, int32_t* num_accesses, int32_t* access_size, int32_t* index_size)
{
    *num_accesses = 2;
    *access_size = 8;
    *index_size = 4;
}

uint8_t PbBackendInsChangeReg(PbInsHandle, PbRegId, PbRegId, uint8_t)
{ return 1; }

void PbBackendInsGetFarPointer(
    PbInsHandle, uint16_t* segment_selector, uint32_t* displacement)
{
    *segment_selector = 0x33u;
    *displacement = UINT32_C(0x12345678);
}

int32_t PbBackendInsInvalid(void) { return 0; }
PbXedDecodedInstHandle PbBackendInsXedDec(PbInsHandle)
{ return reinterpret_cast<PbXedDecodedInstHandle>(static_cast<uintptr_t>(0x7000)); }
PbXedRegId PbBackendInsXedExactMapFromPinReg(PbRegId) { return 10u; }
PbRegId PbBackendInsXedExactMapToPinReg(PbXedRegId) { return PB_REG_RAX; }
PbRegId PbBackendInsXedExactMapToPinRegLegacy(uint32_t) { return PB_REG_RAX; }
void PbBackendPinSetSyntaxAtt(void) {}
void PbBackendPinSetSyntaxIntel(void) {}
void PbBackendPinSetSyntaxXed(void) {}
