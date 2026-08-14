#include "structure_query_backend.h"

#include <cstring>

namespace
{

int32_t g_open_routine;

PbStatus CopyString(
    const char* value, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(std::strlen(value)) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value, static_cast<size_t>(*required_size));
    return PB_OK;
}

} // namespace

uint64_t PbBackendStructureQuery(uint32_t query_id, uint64_t input, uint64_t argument)
{
    return (static_cast<uint64_t>(query_id) << 40u) ^ input ^ argument;
}

PbStatus PbBackendRtnClose(int32_t routine)
{
    if (g_open_routine == 0)
        return PB_ERR_INVALID_STATE;
    if (routine != g_open_routine)
        return PB_ERR_INVALID_ARGUMENT;
    g_open_routine = 0;
    return PB_OK;
}

PbStatus PbBackendRtnCreateAt(
    uint64_t address, const char* name, int32_t* out_routine)
{
    if (g_open_routine != 0)
        return PB_ERR_INVALID_STATE;
    if (address != UINT64_C(0x1234) ||
        std::strcmp(name, "mock_created") != 0)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_routine = 82;
    return PB_OK;
}

int32_t PbBackendRtnFindByAddress(uint64_t address)
{
    return address == UINT64_C(0x1234) ? 81 : 0;
}

int32_t PbBackendRtnFindByName(int32_t image, const char* name)
{
    return image == 76 && std::strcmp(name, "mock_routine") == 0 ? 81 : 0;
}

PbStatus PbBackendRtnFindNameByAddress(
    uint64_t address, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(address == UINT64_C(0x1234) ? "mock_routine" : "",
                      buffer, capacity, required_size);
}

uint64_t PbBackendRtnFunptr(int32_t routine)
{
    return routine == 81 ? UINT64_C(0x1234) : 0;
}

int32_t PbBackendRtnInvalid(void)
{
    return 0;
}

PbStatus PbBackendRtnName(
    int32_t routine, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    if (routine != 81)
        return PB_ERR_INVALID_ARGUMENT;
    return CopyString("mock_routine", buffer, capacity, required_size);
}

PbStatus PbBackendRtnOpen(int32_t routine)
{
    if (g_open_routine != 0)
        return PB_ERR_INVALID_STATE;
    if (routine != 81)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    g_open_routine = routine;
    return PB_OK;
}

PbStatus PbBackendRtnReplace(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (routine != 81 || replacement_address != UINT64_C(0x4000))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = UINT64_C(0x1234);
    return PB_OK;
}

PbStatus PbBackendRtnReplaceProbed(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (routine != 81 || replacement_address != UINT64_C(0x5000))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = UINT64_C(0x2234);
    return PB_OK;
}

PbStatus PbBackendRtnReplaceProbedEx(
    int32_t routine, PbProbeMode mode, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (routine != 81 || mode != 3 ||
        replacement_address != UINT64_C(0x6000))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = UINT64_C(0x3234);
    return PB_OK;
}
