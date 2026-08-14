#ifndef PINBRIDGE_UTILS_CORE_BACKEND_H
#define PINBRIDGE_UTILS_CORE_BACKEND_H

#include <stdint.h>

#include "pinbridge/pinbridge.h"

void* PbBackendAddrintToPointer(uint64_t address);
uint64_t PbBackendAddrintFromString(const char* text);
uint32_t PbBackendBitCount(uint64_t value);
uint8_t PbBackendCharIsSpace(char value);
int32_t PbBackendCharToHexDigit(char value);
char PbBackendCharToUpper(char value);
double PbBackendFlt64FromString(const char* text);
uint64_t PbBackendGetPageOfAddr(uint64_t address);
void PbBackendMemPageRangeAddr(uint64_t address, PbMemRange* out_range);
void PbBackendMemPageRangePointer(const void* pointer, PbMemRange* out_range);
const void* PbBackendGetSp(void);
int32_t PbBackendInt32FromString(const char* text);
int64_t PbBackendInt64FromString(const char* text);
void* PbBackendPtrAtOffset(void* pointer, uint64_t offset);
const void* PbBackendConstPtrAtOffset(const void* pointer, uint64_t offset);
uint64_t PbBackendPtrDiff(const void* pointer1, const void* pointer2);
uint32_t PbBackendUint32FromString(const char* text);
uint64_t PbBackendUint64FromString(const char* text);
uint64_t PbBackendPointerToAddrint(void* pointer);
uint64_t PbBackendConstPointerToAddrint(const void* pointer);
uint64_t PbBackendRoundDownAddr(uint64_t address, uint64_t alignment);
uint64_t PbBackendRoundDownU64(uint64_t value, uint64_t alignment);
uint64_t PbBackendRoundUpAddr(uint64_t address, uint64_t alignment);
uint64_t PbBackendRoundUpU64(uint64_t value, uint64_t alignment);

#endif
