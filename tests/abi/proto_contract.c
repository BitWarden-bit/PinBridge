#include <stddef.h>
#include <stdint.h>

#include "pinbridge/pinbridge.h"

int main(void)
{
    PbProtoArg descriptors[3];
    PbProtoArg invalid_descriptors[3];
    PbProtoHandle proto = PB_PROTO_HANDLE_INVALID;

    if (sizeof(PbCallingStandard) != 4 || sizeof(PbProtoArgKind) != 4 ||
        sizeof(PbProtoArg) != 16 || PB_PROTO_MAX_ARGUMENTS != 8)
        return 1;
    if (PB_CALLINGSTD_INVALID != 0 || PB_CALLINGSTD_DEFAULT != 1 ||
        PB_CALLINGSTD_ART != 5 || PB_PARG_INVALID != 0 ||
        PB_PARG_POINTER != 1 || PB_PARG_VOID != 16 || PB_PARG_END != 19)
        return 2;
    if (pb_proto_arg_for_kind(PB_PARG_UINT, &descriptors[0]) != PB_OK ||
        descriptors[0].kind != PB_PARG_UINT || descriptors[0].size != 4 ||
        descriptors[0].reserved != 0)
        return 3;
    if (pb_proto_arg_enum(4, &descriptors[1]) != PB_OK ||
        descriptors[1].kind != PB_PARG_ENUM || descriptors[1].size != 4 ||
        pb_proto_arg_end(&descriptors[2]) != PB_OK ||
        descriptors[2].kind != PB_PARG_END || descriptors[2].size != 0)
        return 4;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "mock_proto",
            descriptors, 3, &proto) != PB_OK ||
        proto == PB_PROTO_HANDLE_INVALID)
        return 5;
    if (pb_proto_free(proto) != PB_OK)
        return 6;
    if (pb_proto_arg_aggregate(8, &descriptors[0]) != PB_OK ||
        descriptors[0].kind != PB_PARG_AGGREGATE || descriptors[0].size != 8)
        return 7;
    if (pb_proto_arg_for_kind(PB_PARG_INVALID, &descriptors[0]) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_proto_arg_for_kind(PB_PARG_ENUM, &descriptors[0]) !=
            PB_ERR_INVALID_ARGUMENT ||
        pb_proto_arg_aggregate(0, &descriptors[0]) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_arg_enum(0, &descriptors[0]) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_arg_end(0) != PB_ERR_INVALID_ARGUMENT)
        return 8;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_INVALID, "x", descriptors, 3,
            &proto) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, 0, descriptors, 3,
            &proto) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x", 0, 3,
            &proto) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x", descriptors, 0,
            &proto) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x", descriptors,
            PB_PROTO_MAX_ARGUMENTS + 2, &proto) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x", descriptors, 3,
            0) != PB_ERR_INVALID_ARGUMENT ||
        pb_proto_free(PB_PROTO_HANDLE_INVALID) != PB_ERR_INVALID_ARGUMENT)
        return 9;

    invalid_descriptors[0] = descriptors[0];
    invalid_descriptors[1] = descriptors[1];
    invalid_descriptors[2] = descriptors[0];
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 3, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 10;

    invalid_descriptors[0] = descriptors[0];
    invalid_descriptors[1] = descriptors[2];
    invalid_descriptors[2] = descriptors[1];
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 3, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 11;

    invalid_descriptors[0] = descriptors[0];
    invalid_descriptors[0].reserved = 1;
    invalid_descriptors[1] = descriptors[1];
    invalid_descriptors[2] = descriptors[2];
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 3, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 12;

    if (pb_proto_arg_for_kind(PB_PARG_UINT, &invalid_descriptors[0]) != PB_OK)
        return 13;
    invalid_descriptors[0].size = 8;
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 3, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 13;

    if (pb_proto_arg_for_kind(PB_PARG_FLOAT, &invalid_descriptors[0]) !=
            PB_OK ||
        pb_proto_arg_for_kind(PB_PARG_DOUBLE, &invalid_descriptors[1]) !=
            PB_OK ||
        pb_proto_arg_end(&invalid_descriptors[2]) != PB_OK)
        return 14;
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 3, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 15;
    invalid_descriptors[0] = invalid_descriptors[1];
    invalid_descriptors[1] = descriptors[2];
    proto = (PbProtoHandle)(uintptr_t)1;
    if (pb_proto_allocate(
            descriptors[0], PB_CALLINGSTD_DEFAULT, "x",
            invalid_descriptors, 2, &proto) != PB_ERR_INVALID_ARGUMENT ||
        proto != PB_PROTO_HANDLE_INVALID)
        return 16;
    return 0;
}

_Static_assert(sizeof(PbProtoArg) == 16, "PbProtoArg size changed");
_Static_assert(_Alignof(PbProtoArg) == 8, "PbProtoArg alignment changed");
_Static_assert(offsetof(PbProtoArg, kind) == 0, "PbProtoArg kind moved");
_Static_assert(offsetof(PbProtoArg, reserved) == 4,
    "PbProtoArg reserved moved");
_Static_assert(offsetof(PbProtoArg, size) == 8, "PbProtoArg size moved");
