#include "utils_core_backend.h"

#include <cctype>
#include <cstdlib>

void* PbBackendAddrintToPointer(uint64_t address)
{ return reinterpret_cast<void*>(static_cast<uintptr_t>(address)); }
uint64_t PbBackendAddrintFromString(const char* text)
{ return std::strtoull(text, 0, 0); }
uint32_t PbBackendBitCount(uint64_t value)
{
    uint32_t count = 0;
    while (value) { count += static_cast<uint32_t>(value & 1u); value >>= 1u; }
    return count;
}
uint8_t PbBackendCharIsSpace(char value)
{ return std::isspace(static_cast<unsigned char>(value)) ? 1u : 0u; }
int32_t PbBackendCharToHexDigit(char value)
{
    if (value >= '0' && value <= '9') return value - '0';
    if (value >= 'a' && value <= 'f') return value - 'a' + 10;
    if (value >= 'A' && value <= 'F') return value - 'A' + 10;
    return -1;
}
char PbBackendCharToUpper(char value)
{ return static_cast<char>(std::toupper(static_cast<unsigned char>(value))); }
double PbBackendFlt64FromString(const char* text) { return std::strtod(text, 0); }
uint64_t PbBackendGetPageOfAddr(uint64_t address)
{ return address & ~UINT64_C(0xfff); }
void PbBackendMemPageRangeAddr(uint64_t address, PbMemRange* out_range)
{
    out_range->base = address & ~UINT64_C(0xfff);
    out_range->size = UINT64_C(0x1000);
}
void PbBackendMemPageRangePointer(const void* pointer, PbMemRange* out_range)
{ PbBackendMemPageRangeAddr(
    static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pointer)), out_range); }
const void* PbBackendGetSp(void)
{ return reinterpret_cast<const void*>(static_cast<uintptr_t>(0x7000)); }
int32_t PbBackendInt32FromString(const char* text)
{ return static_cast<int32_t>(std::strtol(text, 0, 0)); }
int64_t PbBackendInt64FromString(const char* text)
{ return static_cast<int64_t>(std::strtoll(text, 0, 0)); }
void* PbBackendPtrAtOffset(void* pointer, uint64_t offset)
{ return static_cast<unsigned char*>(pointer) + offset; }
const void* PbBackendConstPtrAtOffset(const void* pointer, uint64_t offset)
{ return static_cast<const unsigned char*>(pointer) + offset; }
uint64_t PbBackendPtrDiff(const void* pointer1, const void* pointer2)
{
    return static_cast<uint64_t>(static_cast<const unsigned char*>(pointer1) -
                                 static_cast<const unsigned char*>(pointer2));
}
uint32_t PbBackendUint32FromString(const char* text)
{ return static_cast<uint32_t>(std::strtoul(text, 0, 0)); }
uint64_t PbBackendUint64FromString(const char* text)
{ return static_cast<uint64_t>(std::strtoull(text, 0, 0)); }
uint64_t PbBackendPointerToAddrint(void* pointer)
{ return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pointer)); }
uint64_t PbBackendConstPointerToAddrint(const void* pointer)
{ return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(pointer)); }
uint64_t PbBackendRoundDownAddr(uint64_t address, uint64_t alignment)
{ return alignment == 0 ? address : (address / alignment) * alignment; }
uint64_t PbBackendRoundDownU64(uint64_t value, uint64_t alignment)
{ return alignment == 0 ? value : (value / alignment) * alignment; }
uint64_t PbBackendRoundUpAddr(uint64_t address, uint64_t alignment)
{ return alignment == 0 ? address : ((address + alignment - 1u) / alignment) * alignment; }
uint64_t PbBackendRoundUpU64(uint64_t value, uint64_t alignment)
{ return alignment == 0 ? value : ((value + alignment - 1u) / alignment) * alignment; }
