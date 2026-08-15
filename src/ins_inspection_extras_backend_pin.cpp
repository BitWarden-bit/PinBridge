#include "pin.H"

#include "ins_inspection_extras_backend.h"
#include "reg_mapping_pin.h"

#include <cstring>
#include <string>

namespace
{

static_assert(sizeof(INS) == sizeof(int32_t), "Pin 3.31 INS layout changed");
static_assert(PB_MEMORY_TYPE_READ == MEMORY_TYPE_READ, "MEMORY_TYPE_READ changed");
static_assert(PB_MEMORY_TYPE_WRITE == MEMORY_TYPE_WRITE, "MEMORY_TYPE_WRITE changed");
static_assert(PB_MEMORY_TYPE_READ2 == MEMORY_TYPE_READ2, "MEMORY_TYPE_READ2 changed");
static_assert(PB_PREDICATE_ALWAYS_TRUE == PREDICATE_ALWAYS_TRUE,
              "PREDICATE_ALWAYS_TRUE changed");
static_assert(PB_PREDICATE_LAST == PREDICATE_LAST, "PREDICATE_LAST changed");
static_assert(PB_VSYSCALL_NR == VSYSCALL_NR, "VSYSCALL_NR changed");

INS ToIns(PbInsHandle value)
{
    INS ins;
    ins.q_set(value.opaque);
    return ins;
}

uint64_t CopyString(const std::string& value, char* buffer, uint64_t capacity)
{
    const uint64_t required = static_cast<uint64_t>(value.size()) + 1u;
    if (buffer && capacity >= required)
        std::memcpy(buffer, value.c_str(), static_cast<size_t>(required));
    return required;
}

PbRegId PublicReg(REG value)
{
    PbRegId out = PB_REG_NONE;
    PbRegIdFromPinReg(value, &out);
    return out;
}

} // namespace

uint8_t PbBackendInsInspectionExtrasIsProbeMode(void)
{
    return PIN_IsProbeMode() ? 1u : 0u;
}

uint64_t PbBackendCategoryStringShort(
    uint32_t value, char* buffer, uint64_t capacity)
{
    return CopyString(CATEGORY_StringShort(value), buffer, capacity);
}

uint64_t PbBackendExtensionStringShort(
    uint32_t value, char* buffer, uint64_t capacity)
{
    return CopyString(EXTENSION_StringShort(value), buffer, capacity);
}

uint64_t PbBackendOpcodeStringShort(
    uint32_t value, char* buffer, uint64_t capacity)
{
    return CopyString(OPCODE_StringShort(value), buffer, capacity);
}

uint64_t PbBackendInsDisassemble(
    PbInsHandle ins, char* buffer, uint64_t capacity)
{
    return CopyString(INS_Disassemble(ToIns(ins)), buffer, capacity);
}

uint64_t PbBackendInsMnemonic(
    PbInsHandle ins, char* buffer, uint64_t capacity)
{
    return CopyString(INS_Mnemonic(ToIns(ins)), buffer, capacity);
}

void PbBackendInsGetNumberAndSizeOfMemAccesses(
    PbInsHandle ins, int32_t* num_accesses, int32_t* access_size,
    int32_t* index_size)
{
    static_assert(sizeof(int) == sizeof(int32_t), "Pin int is not 32-bit");
    GetNumberAndSizeOfMemAccesses(
        ToIns(ins), reinterpret_cast<int*>(num_accesses),
        reinterpret_cast<int*>(access_size), reinterpret_cast<int*>(index_size));
}

uint8_t PbBackendInsChangeReg(
    PbInsHandle ins, PbRegId old_reg, PbRegId new_reg, uint8_t as_read)
{
    REG old_native, new_native;
    if (!PbPinRegFromId(old_reg, &old_native) ||
        !PbPinRegFromId(new_reg, &new_native)) return 0;
    return INS_ChangeReg(ToIns(ins), old_native, new_native, as_read != 0) ? 1u : 0u;
}

void PbBackendInsGetFarPointer(
    PbInsHandle ins, uint16_t* segment_selector, uint32_t* displacement)
{
    INS_GetFarPointer(ToIns(ins), *segment_selector, *displacement);
}

int32_t PbBackendInsInvalid(void) { return INS_Invalid().q(); }

PbXedDecodedInstHandle PbBackendInsXedDec(PbInsHandle ins)
{
    return reinterpret_cast<PbXedDecodedInstHandle>(INS_XedDec(ToIns(ins)));
}

PbXedRegId PbBackendInsXedExactMapFromPinReg(PbRegId pin_reg)
{
    REG native;
    if (!PbPinRegFromId(pin_reg, &native)) return static_cast<PbXedRegId>(0);
    return static_cast<PbXedRegId>(INS_XedExactMapFromPinReg(native));
}

PbRegId PbBackendInsXedExactMapToPinReg(PbXedRegId xed_reg)
{
    return PublicReg(INS_XedExactMapToPinReg(static_cast<xed_reg_enum_t>(xed_reg)));
}

PbRegId PbBackendInsXedExactMapToPinRegLegacy(uint32_t xed_reg)
{
    return PublicReg(INS_XedExactMapToPinReg(static_cast<unsigned int>(xed_reg)));
}

void PbBackendPinSetSyntaxAtt(void) { PIN_SetSyntaxATT(); }
void PbBackendPinSetSyntaxIntel(void) { PIN_SetSyntaxIntel(); }
void PbBackendPinSetSyntaxXed(void) { PIN_SetSyntaxXED(); }
