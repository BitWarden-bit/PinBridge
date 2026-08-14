#include "pin.H"

#include "proto_backend.h"

namespace
{

static_assert(sizeof(PbProtoArg) == 16, "PbProtoArg ABI layout changed");
static_assert(PB_CALLINGSTD_INVALID == CALLINGSTD_INVALID,
              "CALLINGSTD_INVALID value changed");
static_assert(PB_CALLINGSTD_ART == CALLINGSTD_ART,
              "CALLINGSTD_ART value changed");
static_assert(PB_PARG_INVALID == PARG_INVALID, "PARG_INVALID value changed");
static_assert(PB_PARG_END == PARG_END, "PARG_END value changed");

PARG_T ToParg(const PbProtoArg& arg)
{
    PARG_T result;
    result._parg = static_cast<PARG_TYPE>(arg.kind);
    result._size = static_cast<size_t>(arg.size);
    return result;
}

PROTO AllocateByCount(
    PARG_T return_arg, CALLINGSTD_TYPE calling_standard, const char* name,
    const PARG_T* args, uint32_t argument_count)
{
    switch (argument_count)
    {
      case 0:
        return PROTO_Allocate(
            return_arg, calling_standard, name, PIN_PARG_END());
      case 1:
        return PROTO_Allocate(
            return_arg, calling_standard, name, args[0], PIN_PARG_END());
      case 2:
        return PROTO_Allocate(
            return_arg, calling_standard, name,
            args[0], args[1], PIN_PARG_END());
      case 3:
        return PROTO_Allocate(
            return_arg, calling_standard, name,
            args[0], args[1], args[2], PIN_PARG_END());
      case 4:
        return PROTO_Allocate(
            return_arg, calling_standard, name,
            args[0], args[1], args[2], args[3], PIN_PARG_END());
      case 5:
        return PROTO_Allocate(
            return_arg, calling_standard, name,
            args[0], args[1], args[2], args[3], args[4], PIN_PARG_END());
      case 6:
        return PROTO_Allocate(
            return_arg, calling_standard, name, args[0], args[1], args[2],
            args[3], args[4], args[5], PIN_PARG_END());
      case 7:
        return PROTO_Allocate(
            return_arg, calling_standard, name, args[0], args[1], args[2],
            args[3], args[4], args[5], args[6], PIN_PARG_END());
      case 8:
        return PROTO_Allocate(
            return_arg, calling_standard, name, args[0], args[1], args[2],
            args[3], args[4], args[5], args[6], args[7], PIN_PARG_END());
      default:
        return 0;
    }
}

} // namespace

PbStatus PbBackendProtoAllocate(
    PbProtoArg return_arg, PbCallingStandard calling_standard,
    const char* name, const PbProtoArg* descriptors,
    uint32_t descriptor_count, PbProtoHandle* out_proto)
{
    PARG_T args[PB_PROTO_MAX_ARGUMENTS];
    const uint32_t argument_count = descriptor_count - 1u;
    for (uint32_t index = 0; index < argument_count; ++index)
        args[index] = ToParg(descriptors[index]);
    const PROTO proto = AllocateByCount(
        ToParg(return_arg), static_cast<CALLINGSTD_TYPE>(calling_standard),
        name, args, argument_count);
    if (!proto)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_proto = reinterpret_cast<PbProtoHandle>(proto);
    return PB_OK;
}

PbStatus PbBackendProtoFree(PbProtoHandle proto)
{
    PROTO_Free(reinterpret_cast<PROTO>(proto));
    return PB_OK;
}
