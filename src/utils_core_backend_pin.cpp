#include "pin.H"

#include "utils_core_backend.h"

#include <string>

static_assert(sizeof(ADDRINT) == sizeof(uint64_t), "ADDRINT is not 64-bit");
static_assert(sizeof(FLT64) == sizeof(double), "FLT64 is not a C double");
static_assert(sizeof(size_t) == sizeof(uint64_t), "size_t is not 64-bit");

void* PbBackendAddrintToPointer(uint64_t address)
{ return Addrint2VoidStar(static_cast<ADDRINT>(address)); }
uint64_t PbBackendAddrintFromString(const char* text)
{ return static_cast<uint64_t>(AddrintFromString(std::string(text))); }
uint32_t PbBackendBitCount(uint64_t value)
{ return static_cast<uint32_t>(BitCount(static_cast<ADDRINT>(value))); }
uint8_t PbBackendCharIsSpace(char value)
{ return CharIsSpace(value) ? 1u : 0u; }
int32_t PbBackendCharToHexDigit(char value)
{ return static_cast<int32_t>(CharToHexDigit(value)); }
char PbBackendCharToUpper(char value) { return CharToUpper(value); }
double PbBackendFlt64FromString(const char* text)
{ return static_cast<double>(FLT64FromString(std::string(text))); }
uint64_t PbBackendGetPageOfAddr(uint64_t address)
{ return static_cast<uint64_t>(GetPageOfAddr(static_cast<ADDRINT>(address))); }
void PbBackendMemPageRangeAddr(uint64_t address, PbMemRange* out_range)
{
    const MemRange range = MemPageRange(static_cast<ADDRINT>(address));
    out_range->base = static_cast<uint64_t>(VoidStar2Addrint(range.Base()));
    out_range->size = static_cast<uint64_t>(range.Size());
}
void PbBackendMemPageRangePointer(const void* pointer, PbMemRange* out_range)
{
    const MemRange range = MemPageRange(pointer);
    out_range->base = static_cast<uint64_t>(VoidStar2Addrint(range.Base()));
    out_range->size = static_cast<uint64_t>(range.Size());
}
const void* PbBackendGetSp(void) { return GetSp(); }
int32_t PbBackendInt32FromString(const char* text)
{ return static_cast<int32_t>(Int32FromString(std::string(text))); }
int64_t PbBackendInt64FromString(const char* text)
{ return static_cast<int64_t>(Int64FromString(std::string(text))); }
void* PbBackendPtrAtOffset(void* pointer, uint64_t offset)
{ return PtrAtOffset(pointer, static_cast<size_t>(offset)); }
const void* PbBackendConstPtrAtOffset(const void* pointer, uint64_t offset)
{ return PtrAtOffset(pointer, static_cast<size_t>(offset)); }
uint64_t PbBackendPtrDiff(const void* pointer1, const void* pointer2)
{ return static_cast<uint64_t>(PtrDiff(pointer1, pointer2)); }
uint32_t PbBackendUint32FromString(const char* text)
{ return static_cast<uint32_t>(Uint32FromString(std::string(text))); }
uint64_t PbBackendUint64FromString(const char* text)
{ return static_cast<uint64_t>(Uint64FromString(std::string(text))); }
uint64_t PbBackendPointerToAddrint(void* pointer)
{ return static_cast<uint64_t>(VoidStar2Addrint(pointer)); }
uint64_t PbBackendConstPointerToAddrint(const void* pointer)
{ return static_cast<uint64_t>(VoidStar2Addrint(pointer)); }
uint64_t PbBackendRoundDownAddr(uint64_t address, uint64_t alignment)
{ return static_cast<uint64_t>(RoundDown(static_cast<ADDRINT>(address),
                                          static_cast<size_t>(alignment))); }
uint64_t PbBackendRoundDownU64(uint64_t value, uint64_t alignment)
{ return static_cast<uint64_t>(RoundDown(value, static_cast<size_t>(alignment))); }
uint64_t PbBackendRoundUpAddr(uint64_t address, uint64_t alignment)
{ return static_cast<uint64_t>(RoundUp(static_cast<ADDRINT>(address),
                                        static_cast<size_t>(alignment))); }
uint64_t PbBackendRoundUpU64(uint64_t value, uint64_t alignment)
{ return static_cast<uint64_t>(RoundUp(value, static_cast<size_t>(alignment))); }
