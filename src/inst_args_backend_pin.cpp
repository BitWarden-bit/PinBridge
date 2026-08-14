#include "pin.H"

#include "inst_args_backend.h"
#include "regset_conversion_pin.h"

#include <cstdlib>

struct PbIargListOpaque
{
    IARGLIST native;
    REGSET regsets[PB_IARG_LIST_MAX_DESCRIPTORS * 2u];
    uint32_t regset_count;
};

namespace
{

void CopyRegSet(const PbRegSet* source, REGSET* destination)
{
    REGSET_Clear(*destination);
    for (uint32_t reg = static_cast<uint32_t>(REG_FirstInRegset);
         reg <= static_cast<uint32_t>(REG_LastInRegset); ++reg)
    {
        const uint64_t mask = static_cast<uint64_t>(1) << (reg % 64u);
        if ((source->words[reg / 64u] & mask) != 0)
            REGSET_Insert(*destination, static_cast<REG>(reg));
    }
}

bool IsRegType(PbIargType type)
{
    return type == PB_IARG_REG_VALUE || type == PB_IARG_REG_REFERENCE ||
        type == PB_IARG_REG_CONST_REFERENCE || type == PB_IARG_RETURN_REGS;
}

bool IsIndexType(PbIargType type)
{
    return type == PB_IARG_MULTI_ELEMENT_OPERAND ||
        type == PB_IARG_SYSARG_REFERENCE || type == PB_IARG_SYSARG_VALUE ||
        type == PB_IARG_FUNCARG_CALLSITE_REFERENCE ||
        type == PB_IARG_FUNCARG_CALLSITE_VALUE ||
        type == PB_IARG_FUNCARG_ENTRYPOINT_REFERENCE ||
        type == PB_IARG_FUNCARG_ENTRYPOINT_VALUE ||
        type == PB_IARG_MEMORYOP_PTR || type == PB_IARG_MEMORYOP_EA ||
        type == PB_IARG_MEMORYOP_SIZE || type == PB_IARG_MEMORYOP_MASKED_ON;
}

PbStatus AddDescriptor(
    PbIargListOpaque* state, const PbIargDescriptor& descriptor)
{
    const IARG_TYPE type = static_cast<IARG_TYPE>(descriptor.type);
    if (descriptor.type == PB_IARG_ADDRINT)
        IARGLIST_AddArguments(state->native, type,
            static_cast<ADDRINT>(descriptor.value), IARG_END);
    else if (descriptor.type == PB_IARG_PTR)
        IARGLIST_AddArguments(state->native, type,
            reinterpret_cast<VOID*>(static_cast<uintptr_t>(descriptor.value)), IARG_END);
    else if (descriptor.type == PB_IARG_BOOL ||
             descriptor.type == PB_IARG_CHECK_INLINE)
        IARGLIST_AddArguments(state->native, type,
            static_cast<BOOL>(descriptor.value != 0), IARG_END);
    else if (descriptor.type == PB_IARG_UINT32 || IsIndexType(descriptor.type))
        IARGLIST_AddArguments(state->native, type,
            static_cast<UINT32>(descriptor.value), IARG_END);
    else if (descriptor.type == PB_IARG_UINT64)
        IARGLIST_AddArguments(state->native, type,
            static_cast<UINT64>(descriptor.value), IARG_END);
    else if (IsRegType(descriptor.type))
        IARGLIST_AddArguments(state->native, type,
            static_cast<REG>(descriptor.value), IARG_END);
    else if (descriptor.type == PB_IARG_CALL_ORDER)
        IARGLIST_AddArguments(state->native, type,
            static_cast<CALL_ORDER>(descriptor.value), IARG_END);
    else if (descriptor.type == PB_IARG_PROTOTYPE)
        IARGLIST_AddArguments(state->native, type,
            reinterpret_cast<PROTO>(static_cast<uintptr_t>(descriptor.value)), IARG_END);
    else if (descriptor.type == PB_IARG_IARGLIST)
    {
        PbIargListOpaque* nested = reinterpret_cast<PbIargListOpaque*>(
            static_cast<uintptr_t>(descriptor.value));
        IARGLIST_AddArguments(state->native, type, nested->native, IARG_END);
    }
    else if (descriptor.type == PB_IARG_PARTIAL_CONTEXT)
    {
        if (state->regset_count + 2u > PB_IARG_LIST_MAX_DESCRIPTORS * 2u)
            return PB_ERR_OUT_OF_MEMORY;
        REGSET* input = &state->regsets[state->regset_count++];
        REGSET* output = &state->regsets[state->regset_count++];
        CopyRegSet(reinterpret_cast<const PbRegSet*>(
            static_cast<uintptr_t>(descriptor.value)), input);
        CopyRegSet(reinterpret_cast<const PbRegSet*>(
            static_cast<uintptr_t>(descriptor.value2)), output);
        IARGLIST_AddArguments(state->native, type, input, output, IARG_END);
    }
    else if (descriptor.type == PB_IARG_PRESERVE ||
             descriptor.type == PB_IARG_EXPOSE)
    {
        if (state->regset_count == PB_IARG_LIST_MAX_DESCRIPTORS * 2u)
            return PB_ERR_OUT_OF_MEMORY;
        REGSET* registers = &state->regsets[state->regset_count++];
        CopyRegSet(reinterpret_cast<const PbRegSet*>(
            static_cast<uintptr_t>(descriptor.value)), registers);
        IARGLIST_AddArguments(state->native, type, registers, IARG_END);
    }
    else
        IARGLIST_AddArguments(state->native, type, IARG_END);
    return PB_OK;
}

} // namespace

PbStatus PbBackendIargListAlloc(PbIargListHandle* out_list)
{
    PbIargListOpaque* state = static_cast<PbIargListOpaque*>(
        std::malloc(sizeof(PbIargListOpaque)));
    if (!state)
        return PB_ERR_OUT_OF_MEMORY;
    state->native = IARGLIST_Alloc();
    state->regset_count = 0;
    if (!state->native)
    {
        std::free(state);
        return PB_ERR_OUT_OF_MEMORY;
    }
    *out_list = state;
    return PB_OK;
}

PbStatus PbBackendIargListAdd(
    PbIargListHandle list, const PbIargDescriptor* descriptors,
    uint32_t descriptor_count)
{
    for (uint32_t index = 0; index < descriptor_count; ++index)
    {
        const PbStatus status = AddDescriptor(list, descriptors[index]);
        if (status != PB_OK)
            return status;
    }
    return PB_OK;
}

PbStatus PbBackendIargListFree(PbIargListHandle list)
{
    IARGLIST_Free(list->native);
    std::free(list);
    return PB_OK;
}

void* PbBackendIargListNative(PbIargListHandle list)
{
    return list->native;
}
