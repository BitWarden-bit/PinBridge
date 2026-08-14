#include "pin.H"

#include "structure_query_backend.h"

#include <cstring>
#include <string>

namespace
{

static_assert(sizeof(BBL) == sizeof(int32_t), "Pin 3.31 BBL layout changed");
static_assert(sizeof(RTN) == sizeof(int32_t), "Pin 3.31 RTN layout changed");
static_assert(sizeof(SEC) == sizeof(int32_t), "Pin 3.31 SEC layout changed");
static_assert(sizeof(IMG) == sizeof(int32_t), "Pin 3.31 IMG layout changed");
static_assert(sizeof(SYM) == sizeof(int32_t), "Pin 3.31 SYM layout changed");
static_assert(sizeof(TRACE) == sizeof(uintptr_t), "Pin 3.31 TRACE layout changed");
static_assert(PB_PROBE_MODE_DEFAULT == PROBE_MODE_DEFAULT,
              "PROBE_MODE_DEFAULT value drift");
static_assert(PB_PROBE_MODE_ALLOW_RELOCATION == PROBE_MODE_ALLOW_RELOCATION,
              "PROBE_MODE_ALLOW_RELOCATION value drift");
static_assert(PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET ==
                  PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET,
              "PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET value drift");

int32_t g_open_routine;

template< typename T > T ToIndex(uint64_t value)
{
    T result;
    result.q_set(static_cast<int32_t>(value));
    return result;
}

template< typename T > uint64_t ToBits(T value) { return static_cast<uint64_t>(value); }
uint64_t ToBits(INS value) { return static_cast<uint32_t>(value.q()); }
uint64_t ToBits(BBL value) { return static_cast<uint32_t>(value.q()); }
uint64_t ToBits(RTN value) { return static_cast<uint32_t>(value.q()); }
uint64_t ToBits(SEC value) { return static_cast<uint32_t>(value.q()); }
uint64_t ToBits(IMG value) { return static_cast<uint32_t>(value.q()); }
uint64_t ToBits(SYM value) { return static_cast<uint32_t>(value.q()); }

AFUNPTR ToAfunptr(uint64_t address)
{
    return reinterpret_cast<AFUNPTR>(static_cast<uintptr_t>(address));
}

uint64_t FromAfunptr(AFUNPTR function)
{
    return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(function));
}

UINT PbSafeRtnDynamicMethodId(RTN routine)
{
    return RTN_IsDynamic(routine) ? RTN_DynamicMethodId(routine) : 0;
}

#define PB_PIN_INPUT_BBL(value) ToIndex<BBL>(value)
#define PB_PIN_INPUT_TRACE(value) reinterpret_cast<TRACE>(static_cast<uintptr_t>(value))
#define PB_PIN_INPUT_RTN(value) ToIndex<RTN>(value)
#define PB_PIN_INPUT_SEC(value) ToIndex<SEC>(value)
#define PB_PIN_INPUT_IMG(value) ToIndex<IMG>(value)
#define PB_PIN_ARG_UINT32(value) static_cast<UINT32>(value)
#define PB_PIN_ARG_PROBE_MODE(value) static_cast<PROBE_MODE>(value)
#define PB_PIN_ARG_IMG_PROPERTY(value) static_cast<IMG_PROPERTY>(value)

PbStatus CopyString(
    const std::string& value, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(value.size()) + 1u;
    if (!buffer || capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    std::memcpy(buffer, value.c_str(), static_cast<size_t>(*required_size));
    return PB_OK;
}

} // namespace

uint64_t PbBackendStructureQuery(uint32_t query_id, uint64_t input, uint64_t argument)
{
    switch (query_id)
    {
#define PB_HANDLE_QUERY0(input_kind, return_kind, c_symbol, pin_symbol, api_id) \
    case PB_STRUCTURE_QUERY_ID_##c_symbol: return ToBits(pin_symbol(PB_PIN_INPUT_##input_kind(input)));
#define PB_HANDLE_QUERY1(input_kind, return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    case PB_STRUCTURE_QUERY_ID_##c_symbol: \
        return ToBits(pin_symbol(PB_PIN_INPUT_##input_kind(input), PB_PIN_ARG_##argument_kind(argument)));
#include "pinbridge/generated/structure_queries.inc"
#undef PB_HANDLE_QUERY1
#undef PB_HANDLE_QUERY0
    default: return 0;
    }
}

PbStatus PbBackendRtnClose(int32_t routine)
{
    if (g_open_routine == 0)
        return PB_ERR_INVALID_STATE;
    if (routine != g_open_routine)
        return PB_ERR_INVALID_ARGUMENT;
    RTN_Close(ToIndex<RTN>(static_cast<uint32_t>(routine)));
    g_open_routine = 0;
    return PB_OK;
}

PbStatus PbBackendRtnCreateAt(
    uint64_t address, const char* name, int32_t* out_routine)
{
    if (g_open_routine != 0)
        return PB_ERR_INVALID_STATE;
    const RTN routine = RTN_CreateAt(
        static_cast<ADDRINT>(address), std::string(name));
    if (!RTN_Valid(routine))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_routine = routine.q();
    return PB_OK;
}

int32_t PbBackendRtnFindByAddress(uint64_t address)
{
    return RTN_FindByAddress(static_cast<ADDRINT>(address)).q();
}

int32_t PbBackendRtnFindByName(int32_t image, const char* name)
{
    return RTN_FindByName(ToIndex<IMG>(static_cast<uint32_t>(image)), name).q();
}

PbStatus PbBackendRtnFindNameByAddress(
    uint64_t address, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(
        RTN_FindNameByAddress(static_cast<ADDRINT>(address)),
        buffer, capacity, required_size);
}

uint64_t PbBackendRtnFunptr(int32_t routine)
{
    return static_cast<uint64_t>(reinterpret_cast<uintptr_t>(
        RTN_Funptr(ToIndex<RTN>(static_cast<uint32_t>(routine)))));
}

int32_t PbBackendRtnInvalid(void)
{
    return RTN_Invalid().q();
}

PbStatus PbBackendRtnName(
    int32_t routine, char* buffer, uint64_t capacity,
    uint64_t* required_size)
{
    return CopyString(
        RTN_Name(ToIndex<RTN>(static_cast<uint32_t>(routine))),
        buffer, capacity, required_size);
}

PbStatus PbBackendRtnOpen(int32_t routine)
{
    if (g_open_routine != 0)
        return PB_ERR_INVALID_STATE;
    const RTN pin_routine = ToIndex<RTN>(static_cast<uint32_t>(routine));
    if (!RTN_Valid(pin_routine))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    RTN_Open(pin_routine);
    g_open_routine = routine;
    return PB_OK;
}

PbStatus PbBackendRtnReplace(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const RTN pin_routine = ToIndex<RTN>(static_cast<uint32_t>(routine));
    if (!RTN_Valid(pin_routine))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    const AFUNPTR original =
        RTN_Replace(pin_routine, ToAfunptr(replacement_address));
    if (!original)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = FromAfunptr(original);
    return PB_OK;
}

PbStatus PbBackendRtnReplaceProbed(
    int32_t routine, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const RTN pin_routine = ToIndex<RTN>(static_cast<uint32_t>(routine));
    if (!RTN_Valid(pin_routine) ||
        !RTN_IsSafeForProbedReplacement(pin_routine))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    const AFUNPTR original =
        RTN_ReplaceProbed(pin_routine, ToAfunptr(replacement_address));
    if (!original)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = FromAfunptr(original);
    return PB_OK;
}

PbStatus PbBackendRtnReplaceProbedEx(
    int32_t routine, PbProbeMode mode, uint64_t replacement_address,
    uint64_t* out_original_address)
{
    if (!PIN_IsProbeMode())
        return PB_ERR_INVALID_STATE;
    const RTN pin_routine = ToIndex<RTN>(static_cast<uint32_t>(routine));
    const PROBE_MODE pin_mode = static_cast<PROBE_MODE>(mode);
    if (!RTN_Valid(pin_routine) ||
        !RTN_IsSafeForProbedReplacementEx(pin_routine, pin_mode))
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    const AFUNPTR original = RTN_ReplaceProbedEx(
        pin_routine, pin_mode, ToAfunptr(replacement_address));
    if (!original)
        return PB_ERR_PIN_REJECTED_ARGUMENTS;
    *out_original_address = FromAfunptr(original);
    return PB_OK;
}
