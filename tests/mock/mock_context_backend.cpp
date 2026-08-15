#include "context_backend.h"

#include <cstdlib>
#include <cstring>

namespace
{

const uint32_t kSlotCount = 32;
const uint32_t kSlotSize = 32;
const uint32_t kContextSize = 2048;
const uint32_t kFpStateOffset = 1024;
const uint64_t kFpStateSize = 64;
const uint32_t kFxSaveOffset = 1088;
const uint32_t kStackArgsOffset = 1600;
const uint32_t kStackArgCount = 32;

uint64_t RegSize(PbRegId reg)
{
    if (reg >= kSlotCount)
        return 0;
    return reg == 2 ? 16u : 8u;
}

uint8_t* Slot(void* context, PbRegId reg)
{
    return static_cast<uint8_t*>(context) + reg * kSlotSize;
}

const uint8_t* Slot(const void* context, PbRegId reg)
{
    return static_cast<const uint8_t*>(context) + reg * kSlotSize;
}

} // namespace

PbStatus PbBackendGetContextFpState(
    const void* context, uint8_t* buffer, uint64_t capacity, uint64_t* required_size)
{
    *required_size = kFpStateSize;
    if (capacity < kFpStateSize)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, static_cast<const uint8_t*>(context) + kFpStateOffset,
                static_cast<size_t>(kFpStateSize));
    return PB_OK;
}

PbStatus PbBackendSetContextFpState(
    void* context, const uint8_t* value, uint64_t value_size)
{
    if (value_size != kFpStateSize)
        return PB_ERR_INVALID_ARGUMENT;
    std::memcpy(static_cast<uint8_t*>(context) + kFpStateOffset, value,
                static_cast<size_t>(kFpStateSize));
    return PB_OK;
}

PbStatus PbBackendGetContextFxSave(const void* context, PbFxSave* out_fxsave)
{
    std::memcpy(out_fxsave->bytes, static_cast<const uint8_t*>(context) + kFxSaveOffset,
                PB_FXSAVE_SIZE);
    return PB_OK;
}

PbStatus PbBackendSetContextFxSave(void* context, const PbFxSave* fxsave)
{
    std::memcpy(static_cast<uint8_t*>(context) + kFxSaveOffset, fxsave->bytes,
                PB_FXSAVE_SIZE);
    return PB_OK;
}

PbStatus PbBackendGetFullContextRegsSet(PbRegSet* out_regs)
{
    std::memset(out_regs, 0, sizeof(*out_regs));
    out_regs->words[0] = UINT64_C(0x6);
    return PB_OK;
}

PbStatus PbBackendSupportsProcessorState(PbProcessorState state, uint8_t* out_supported)
{
    *out_supported = state <= PB_PROCESSOR_STATE_YMM ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendContextContainsState(
    void*, PbProcessorState state, uint8_t* out_contains)
{
    *out_contains = state <= PB_PROCESSOR_STATE_YMM ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendGetContextRegval(
    const void* context, PbRegId reg, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    const uint64_t size = RegSize(reg);
    if (size == 0)
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = size;
    if (capacity < size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, Slot(context, reg), static_cast<size_t>(size));
    return PB_OK;
}

PbStatus PbBackendSaveContext(const void* source, void* destination)
{
    std::memcpy(destination, source, kContextSize);
    return PB_OK;
}

PbStatus PbBackendSetContextReg(void* context, PbRegId reg, uint64_t value)
{
    if (RegSize(reg) == 0)
        return PB_ERR_INVALID_ARGUMENT;
    std::memcpy(Slot(context, reg), &value, sizeof(value));
    return PB_OK;
}

PbStatus PbBackendSetContextRegval(
    void* context, PbRegId reg, const uint8_t* value, uint64_t value_size)
{
    const uint64_t size = RegSize(reg);
    if (size == 0 || value_size != size)
        return PB_ERR_INVALID_ARGUMENT;
    std::memcpy(Slot(context, reg), value, static_cast<size_t>(size));
    return PB_OK;
}

PbStatus PbBackendGetContextStackArg(
    const void* context, uint32_t index, uint64_t* out_value)
{
    if (index >= kStackArgCount)
        return PB_ERR_INVALID_ARGUMENT;
    std::memcpy(out_value,
                static_cast<const uint8_t*>(context) + kStackArgsOffset + index * sizeof(uint64_t),
                sizeof(*out_value));
    return PB_OK;
}

PbStatus PbBackendSetContextStackArg(void* context, uint32_t index, uint64_t value)
{
    if (index >= kStackArgCount)
        return PB_ERR_INVALID_ARGUMENT;
    std::memcpy(static_cast<uint8_t*>(context) + kStackArgsOffset + index * sizeof(uint64_t),
                &value, sizeof(value));
    return PB_OK;
}

PB_NORETURN void PbBackendExecuteAt(const void*)
{
    std::abort();
}
