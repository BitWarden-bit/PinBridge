#include "pin.H"

#include "context_backend.h"
#include "reg_mapping_pin.h"
#include "regset_conversion_pin.h"

#include <cstring>

namespace
{

static_assert(sizeof(FXSAVE) == PB_FXSAVE_SIZE,
              "PbFxSave size differs from this Pin SDK");
static_assert(FPSTATE_ALIGNMENT == 64, "unexpected Pin FPSTATE alignment");

bool RegBufferSize(PbRegId value, uint64_t* size)
{
    REG reg;
    if (!PbPinRegFromId(value, &reg))
        return false;
    const UINT32 pin_size = REG_Size(reg);
    if (pin_size == 0)
        return false;
    *size = static_cast<uint64_t>(pin_size);
    return true;
}

bool ScalarContextReg(PbRegId value)
{
    REG reg;
    if (!PbPinRegFromId(value, &reg))
        return false;
    return REG_valid_for_iarg_reg_value(reg) || REG_is_fr_for_get_context(reg);
}

/* Write path gate. The read-oriented check above rejects registers that are
   not valid IARG_REG_VALUE / get-context scalars but are still perfectly
   settable via PIN_SetContextReg — REG_INST_PTR (needed to redirect
   execution through writeable/stopped contexts) and the flags register
   (REG_RFLAGS on intel64, REG_EFLAGS on ia32; both alias REG_GFLAGS). */
bool WriteableContextReg(PbRegId value)
{
    REG reg;
    if (!PbPinRegFromId(value, &reg))
        return false;
    return REG_valid_for_iarg_reg_value(reg) || REG_is_fr_for_get_context(reg) ||
           reg == REG_INST_PTR || reg == REG_GFLAGS;
}

bool StackArgAddress(const CONTEXT* context, uint32_t index, ADDRINT* out_address)
{
    if (!context || !out_address)
        return false;
#if defined(TARGET_IA32E)
    const REG stack_reg = REG_RSP;
    const uint64_t offset = 0x28ull + static_cast<uint64_t>(index) * 8ull;
#else
    const REG stack_reg = REG_ESP;
    const uint64_t offset = 4ull + static_cast<uint64_t>(index) * 4ull;
#endif
    const uint64_t stack = static_cast<uint64_t>(
        PIN_GetContextReg(context, stack_reg));
    if (offset > static_cast<uint64_t>(~static_cast<ADDRINT>(0)) - stack)
        return false;
    *out_address = static_cast<ADDRINT>(stack + offset);
    return true;
}

} // namespace

PbStatus PbBackendGetFullContextRegsSet(PbRegSet* out_regs)
{
    const REGSET direct = PIN_GetFullContextRegsSet();
    PbPinRegSetConversion::FromPin(direct, out_regs);
    return PB_OK;
}

PbStatus PbBackendGetContextFpState(
    const void* context, uint8_t* buffer, uint64_t capacity, uint64_t* required_size)
{
    *required_size = static_cast<uint64_t>(FPSTATE_SIZE);
    if (capacity < *required_size)
        return PB_ERR_BUFFER_TOO_SMALL;
    alignas(FPSTATE_ALIGNMENT) FPSTATE state = {};
    PIN_GetContextFPState(static_cast<const CONTEXT*>(context), &state);
    std::memcpy(buffer, &state, FPSTATE_SIZE);
    return PB_OK;
}

PbStatus PbBackendSetContextFpState(
    void* context, const uint8_t* value, uint64_t value_size)
{
    if (value_size != static_cast<uint64_t>(FPSTATE_SIZE))
        return PB_ERR_INVALID_ARGUMENT;
    alignas(FPSTATE_ALIGNMENT) FPSTATE state = {};
    std::memcpy(&state, value, FPSTATE_SIZE);
    PIN_SetContextFPState(static_cast<CONTEXT*>(context), &state);
    return PB_OK;
}

PbStatus PbBackendGetContextFxSave(const void* context, PbFxSave* out_fxsave)
{
    FXSAVE state = {};
    PIN_GetContextFXSave(static_cast<const CONTEXT*>(context), &state);
    std::memcpy(out_fxsave->bytes, &state, sizeof(state));
    return PB_OK;
}

PbStatus PbBackendSetContextFxSave(void* context, const PbFxSave* fxsave)
{
    FXSAVE state = {};
    std::memcpy(&state, fxsave->bytes, sizeof(state));
    PIN_SetContextFXSave(static_cast<CONTEXT*>(context), &state);
    return PB_OK;
}

PbStatus PbBackendSupportsProcessorState(PbProcessorState state, uint8_t* out_supported)
{
    *out_supported = PIN_SupportsProcessorState(static_cast<PROCESSOR_STATE>(state)) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendContextContainsState(
    void* context, PbProcessorState state, uint8_t* out_contains)
{
    *out_contains = PIN_ContextContainsState(
        static_cast<CONTEXT*>(context), static_cast<PROCESSOR_STATE>(state)) ? 1u : 0u;
    return PB_OK;
}

PbStatus PbBackendGetContextRegval(
    const void* context, PbRegId reg, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size)
{
    uint64_t size = 0;
    if (!RegBufferSize(reg, &size))
        return PB_ERR_INVALID_ARGUMENT;
    *required_size = size;
    if (capacity < size)
        return PB_ERR_BUFFER_TOO_SMALL;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg))
        return PB_ERR_INVALID_ARGUMENT;
    PIN_GetContextRegval(static_cast<const CONTEXT*>(context), native_reg, buffer);
    return PB_OK;
}

PbStatus PbBackendSaveContext(const void* source, void* destination)
{
    PIN_SaveContext(
        static_cast<const CONTEXT*>(source), static_cast<CONTEXT*>(destination));
    return PB_OK;
}

PbStatus PbBackendSetContextReg(void* context, PbRegId reg, uint64_t value)
{
    if (!WriteableContextReg(reg))
        return PB_ERR_INVALID_ARGUMENT;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg))
        return PB_ERR_INVALID_ARGUMENT;
    PIN_SetContextReg(static_cast<CONTEXT*>(context), native_reg,
                      static_cast<ADDRINT>(value));
    return PB_OK;
}

PbStatus PbBackendSetContextRegval(
    void* context, PbRegId reg, const uint8_t* value, uint64_t value_size)
{
    uint64_t size = 0;
    if (!RegBufferSize(reg, &size) || value_size != size)
        return PB_ERR_INVALID_ARGUMENT;
    REG native_reg;
    if (!PbPinRegFromId(reg, &native_reg))
        return PB_ERR_INVALID_ARGUMENT;
    PIN_SetContextRegval(static_cast<CONTEXT*>(context), native_reg, value);
    return PB_OK;
}

PbStatus PbBackendGetContextStackArg(
    const void* context, uint32_t index, uint64_t* out_value)
{
    if (!out_value)
        return PB_ERR_INVALID_ARGUMENT;
    ADDRINT address = 0;
    if (!StackArgAddress(static_cast<const CONTEXT*>(context), index, &address))
        return PB_ERR_INVALID_ARGUMENT;
    ADDRINT value = 0;
    if (PIN_SafeCopy(&value, reinterpret_cast<const VOID*>(address), sizeof(value))
        != sizeof(value))
        return PB_ERR_INVALID_STATE;
    *out_value = static_cast<uint64_t>(value);
    return PB_OK;
}

PbStatus PbBackendSetContextStackArg(
    void* context, uint32_t index, uint64_t value)
{
    ADDRINT address = 0;
    if (!StackArgAddress(static_cast<const CONTEXT*>(context), index, &address))
        return PB_ERR_INVALID_ARGUMENT;
    const ADDRINT narrowed = static_cast<ADDRINT>(value);
    if (PIN_SafeCopy(reinterpret_cast<VOID*>(address), &narrowed, sizeof(narrowed))
        != sizeof(narrowed))
        return PB_ERR_INVALID_STATE;
    return PB_OK;
}

PB_NORETURN void PbBackendExecuteAt(const void* context)
{
    PIN_ExecuteAt(static_cast<const CONTEXT*>(context));
}
