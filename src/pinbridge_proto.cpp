#include "pinbridge/pinbridge.h"

#include "proto_backend.h"

#include <cstddef>

namespace
{

template< typename Function > PbStatus GuardProtoOperation(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try { return function(); }
    catch (...) { return PB_ERR_INTERNAL; }
#else
    return function();
#endif
}

void SetArg(PbProtoArgKind kind, uint64_t size, PbProtoArg* out_arg)
{
    out_arg->kind = kind;
    out_arg->reserved = 0;
    out_arg->size = size;
}

uint64_t PrimitiveSize(PbProtoArgKind kind)
{
    switch (kind)
    {
      case PB_PARG_POINTER: return sizeof(void*);
      case PB_PARG_BOOL: return sizeof(bool);
      case PB_PARG_CHAR: return sizeof(char);
      case PB_PARG_UCHAR: return sizeof(unsigned char);
      case PB_PARG_SCHAR: return sizeof(signed char);
      case PB_PARG_SHORT: return sizeof(short);
      case PB_PARG_USHORT: return sizeof(unsigned short);
      case PB_PARG_INT: return sizeof(int);
      case PB_PARG_UINT: return sizeof(unsigned int);
      case PB_PARG_LONG: return sizeof(long);
      case PB_PARG_ULONG: return sizeof(unsigned long);
      case PB_PARG_LONGLONG: return sizeof(long long);
      case PB_PARG_ULONGLONG: return sizeof(unsigned long long);
      case PB_PARG_FLOAT: return sizeof(float);
      case PB_PARG_DOUBLE: return sizeof(double);
      case PB_PARG_VOID: return 0;
      default: return UINT64_MAX;
    }
}

bool IsSizedDescriptorValid(const PbProtoArg& arg)
{
    if (arg.reserved != 0)
        return false;
    const uint64_t primitive_size = PrimitiveSize(arg.kind);
    if (primitive_size != UINT64_MAX)
        return arg.size == primitive_size;
    if (arg.kind == PB_PARG_ENUM || arg.kind == PB_PARG_AGGREGATE)
        return arg.size > 0 && arg.size <= sizeof(uint64_t);
    return false;
}

bool IsArgumentValid(const PbProtoArg& arg)
{
    return arg.kind != PB_PARG_VOID && arg.kind != PB_PARG_FLOAT &&
        arg.kind != PB_PARG_DOUBLE && IsSizedDescriptorValid(arg);
}

} // namespace

PbStatus PB_CALL pb_proto_arg_for_kind(
    PbProtoArgKind kind, PbProtoArg* out_arg)
{
    if (!out_arg)
        return PB_ERR_INVALID_ARGUMENT;
    const uint64_t size = PrimitiveSize(kind);
    if (size == UINT64_MAX)
        return PB_ERR_INVALID_ARGUMENT;
    SetArg(kind, size, out_arg);
    return PB_OK;
}

PbStatus PB_CALL pb_proto_arg_aggregate(uint64_t size, PbProtoArg* out_arg)
{
    if (!out_arg || size == 0 || size > sizeof(uint64_t))
        return PB_ERR_INVALID_ARGUMENT;
    SetArg(PB_PARG_AGGREGATE, size, out_arg);
    return PB_OK;
}

PbStatus PB_CALL pb_proto_arg_enum(uint64_t size, PbProtoArg* out_arg)
{
    if (!out_arg || size == 0 || size > sizeof(uint64_t))
        return PB_ERR_INVALID_ARGUMENT;
    SetArg(PB_PARG_ENUM, size, out_arg);
    return PB_OK;
}

PbStatus PB_CALL pb_proto_arg_end(PbProtoArg* out_arg)
{
    if (!out_arg)
        return PB_ERR_INVALID_ARGUMENT;
    SetArg(PB_PARG_END, 0, out_arg);
    return PB_OK;
}

PbStatus PB_CALL pb_proto_allocate(
    PbProtoArg return_arg, PbCallingStandard calling_standard,
    const char* name, const PbProtoArg* descriptors,
    uint32_t descriptor_count, PbProtoHandle* out_proto)
{
    if (calling_standard <= PB_CALLINGSTD_INVALID ||
        calling_standard > PB_CALLINGSTD_ART || !name || !descriptors ||
        descriptor_count == 0 ||
        descriptor_count > PB_PROTO_MAX_ARGUMENTS + 1u || !out_proto ||
        !IsSizedDescriptorValid(return_arg))
        return PB_ERR_INVALID_ARGUMENT;
    *out_proto = PB_PROTO_HANDLE_INVALID;
    for (uint32_t index = 0; index + 1u < descriptor_count; ++index)
    {
        if (!IsArgumentValid(descriptors[index]))
            return PB_ERR_INVALID_ARGUMENT;
    }
    const PbProtoArg& end = descriptors[descriptor_count - 1u];
    if (end.kind != PB_PARG_END || end.reserved != 0 || end.size != 0)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardProtoOperation([&]() {
        return PbBackendProtoAllocate(
            return_arg, calling_standard, name, descriptors,
            descriptor_count, out_proto);
    });
}

PbStatus PB_CALL pb_proto_free(PbProtoHandle proto)
{
    if (proto == PB_PROTO_HANDLE_INVALID)
        return PB_ERR_INVALID_ARGUMENT;
    return GuardProtoOperation([&]() { return PbBackendProtoFree(proto); });
}
