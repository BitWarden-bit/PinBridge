#include "pinbridge/pinbridge.h"

#include "inst_args_backend.h"

namespace
{

template< typename Function > PbStatus GuardIargOperation(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

bool IsSupportedType(PbIargType type)
{
    switch (type)
    {
      case PB_IARG_INVALID:
      case PB_IARG_PREDICATE:
      case PB_IARG_STACK_VALUE:
      case PB_IARG_STACK_REFERENCE:
      case PB_IARG_MEMORY_VALUE:
      case PB_IARG_MEMORY_REFERENCE:
      case PB_IARG_FILE_NAME:
      case PB_IARG_LINE_NO:
      case PB_IARG_LAST:
        return false;
      default:
        return type > PB_IARG_INVALID && type < PB_IARG_LAST;
    }
}

bool IsDescriptorValid(const PbIargDescriptor& descriptor)
{
    if (descriptor.reserved != 0 || !IsSupportedType(descriptor.type))
        return false;
    if (descriptor.type == PB_IARG_PARTIAL_CONTEXT)
        return descriptor.value != 0 && descriptor.value2 != 0;
    if (descriptor.value2 != 0)
        return false;
    if (descriptor.type == PB_IARG_PROTOTYPE ||
        descriptor.type == PB_IARG_PRESERVE ||
        descriptor.type == PB_IARG_IARGLIST ||
        descriptor.type == PB_IARG_EXPOSE)
        return descriptor.value != 0;
    return true;
}

} // namespace

PbStatus PB_CALL pb_iarg_list_alloc(PbIargListHandle* out_list)
{
    if (!out_list)
        return PB_ERR_INVALID_ARGUMENT;
    *out_list = PB_IARG_LIST_INVALID;
    return GuardIargOperation(
        [&]() { return PbBackendIargListAlloc(out_list); });
}

PbStatus PB_CALL pb_iarg_list_add(
    PbIargListHandle list, const PbIargDescriptor* descriptors,
    uint32_t descriptor_count)
{
    if (list == PB_IARG_LIST_INVALID || !descriptors || descriptor_count == 0 ||
        descriptor_count > PB_IARG_LIST_MAX_DESCRIPTORS)
        return PB_ERR_INVALID_ARGUMENT;
    for (uint32_t index = 0; index < descriptor_count; ++index)
    {
        if (!IsDescriptorValid(descriptors[index]))
            return PB_ERR_INVALID_ARGUMENT;
    }
    return GuardIargOperation([&]() {
        return PbBackendIargListAdd(list, descriptors, descriptor_count);
    });
}

PbStatus PB_CALL pb_iarg_list_free(PbIargListHandle list)
{
    if (list == PB_IARG_LIST_INVALID)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardIargOperation([&]() { return PbBackendIargListFree(list); });
}
