#ifndef PINBRIDGE_PINBRIDGE_H
#define PINBRIDGE_PINBRIDGE_H

#include <stdint.h>

#if defined(_WIN32)
#  if defined(PB_STATIC)
#    define PB_API
#  elif defined(PB_BUILDING_DLL)
#    define PB_API __declspec(dllexport)
#  else
#    define PB_API __declspec(dllimport)
#  endif
#  define PB_CALL __cdecl
#  define PB_NORETURN __declspec(noreturn)
#else
#  if defined(PB_BUILDING_DLL)
#    define PB_API __attribute__((visibility("default")))
#  else
#    define PB_API
#  endif
#  define PB_CALL
#  define PB_NORETURN __attribute__((noreturn))
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define PB_ABI_VERSION_MAJOR 1u
#define PB_ABI_VERSION_MINOR 9u
#define PB_ABI_VERSION ((PB_ABI_VERSION_MAJOR << 16u) | PB_ABI_VERSION_MINOR)

typedef int32_t PbStatus;

typedef int32_t PbTri;
#define PB_TRI_YES ((PbTri)0)
#define PB_TRI_NO ((PbTri)1)
#define PB_TRI_MAYBE ((PbTri)2)

#define PB_OK ((PbStatus)0)
#define PB_ERR_INVALID_ARGUMENT ((PbStatus)1)
#define PB_ERR_BUFFER_TOO_SMALL ((PbStatus)2)
#define PB_ERR_OUT_OF_MEMORY ((PbStatus)3)
#define PB_ERR_UNSUPPORTED ((PbStatus)4)
#define PB_ERR_PIN_REJECTED_ARGUMENTS ((PbStatus)5)
#define PB_ERR_INVALID_STATE ((PbStatus)6)
#define PB_ERR_INTERNAL ((PbStatus)7)

/* SDK build identity for the exact Pin 3.31 kit used by this ABI snapshot. */
#include "pinbridge/generated/version_constants.inc"

/* Borrowed instruction token. It is only valid in the Pin callback window. */
typedef struct PbInsHandle
{
    int32_t opaque;
} PbInsHandle;

/* Borrowed routine token returned by an INS inspection query. */
typedef struct PbRtnHandle
{
    int32_t opaque;
} PbRtnHandle;

typedef struct PbBblHandle
{
    int32_t opaque;
} PbBblHandle;

typedef struct PbSecHandle
{
    int32_t opaque;
} PbSecHandle;

typedef struct PbImgHandle
{
    int32_t opaque;
} PbImgHandle;

typedef struct PbSymHandle
{
    int32_t opaque;
} PbSymHandle;

/* TRACE is a borrowed pointer in Pin 3.31, not an INDEX token. */
typedef struct PbTraceOpaque* PbTraceHandle;

/* Opaque callback registration or scoped-handler token. Zero is invalid. */
typedef struct PbCallbackHandle
{
    uint64_t opaque;
} PbCallbackHandle;

#define PB_CALLBACK_INVALID_OPAQUE UINT64_C(0)

/* Borrowed CONTEXT pointer. Consumers must not dereference or retain it. */
typedef struct PbContextOpaque* PbContextHandle;
typedef const struct PbContextOpaque* PbConstContextHandle;

/* Borrowed physical register context. Consumers must not dereference or retain it. */
typedef struct PbPhysicalContextOpaque* PbPhysicalContextHandle;
typedef const struct PbPhysicalContextOpaque* PbConstPhysicalContextHandle;

/* Opaque Pin-owned global configuration. There is no SDK destroy operation. */
typedef struct PbPinConfigurationOpaque* PbPinConfigurationHandle;

/* Borrowed XED decode object. Valid only while PbXedDecodeCallback is running. */
typedef struct PbXedDecodedInstOpaque* PbXedDecodedInstHandle;

/* XED pre-decode feature bits. Only explicitly selected bits are changed. */
#define PB_XED_DECODE_FEATURE_CET UINT32_C(0x1)
#define PB_XED_DECODE_FEATURE_CLDEMOTE UINT32_C(0x2)
#define PB_XED_DECODE_FEATURE_MPX UINT32_C(0x4)
#define PB_XED_DECODE_FEATURE_ALL UINT32_C(0x7)

/* Borrowed, logically opaque Pin exception info. NULL means details are not requested. */
typedef struct PbExceptionInfoOpaque* PbExceptionInfoHandle;

/* Owning synchronization handles. Release each with its matching fini/destroy API. */
typedef struct PbLockOpaque* PbLockHandle;
typedef struct PbMutexOpaque* PbMutexHandle;
typedef struct PbRwMutexOpaque* PbRwMutexHandle;
typedef struct PbSemaphoreOpaque* PbSemaphoreHandle;

/* Borrowed child-process handle. Valid only while PbFollowChildProcessCallback runs. */
typedef struct PbChildProcessOpaque* PbChildProcessHandle;

/* Numeric identities from the matched Pin 3.31 SDK build. */
typedef uint32_t PbRegId;
typedef uint32_t PbRegName;
typedef uint32_t PbRegWidth;
typedef uint32_t PbRegAccess;
typedef uint32_t PbRegAllocType;
typedef uint32_t PbRegClass;
typedef uint32_t PbRegSubclass;
typedef uint64_t PbRegClassBits;
typedef uint32_t PbIargType;
typedef int32_t PbCallOrder;
typedef uint32_t PbPinErrorType;
typedef uint32_t PbPinErrorSeverity;
typedef uint32_t PbProcessorState;
typedef uint32_t PbSecType;
typedef uint32_t PbThreadId;
typedef uint32_t PbOsProcessId;
typedef uint32_t PbOsThreadId;
typedef uint64_t PbPinThreadUid;
typedef int32_t PbTlsKey;
typedef uint32_t PbAttachStatus;
typedef uint32_t PbContextChangeReason;
typedef uint32_t PbExceptHandlingResult;
typedef uint32_t PbForkPoint;
typedef uint32_t PbCallbackType;
typedef uint32_t PbReplayMode;
typedef uint32_t PbSmcMode;
typedef uint32_t PbSymbolInfoMode;
typedef uint32_t PbIpoint;
typedef uint32_t PbBufferId;
typedef uint32_t PbImgProperty;
typedef uint32_t PbImgType;
typedef uint32_t PbSyscallStandard;
typedef uint32_t PbUndecoration;
typedef uint32_t PbCallingStandard;
typedef uint32_t PbProtoArgKind;
typedef uint32_t PbMemoryType;
typedef uint32_t PbPredicate;
typedef uint32_t PbXedRegId;
typedef uint32_t PbProbeMode;
typedef uint32_t PbPinMemop;
typedef uint32_t PbPinOpElementAccess;
typedef uint32_t PbLogType;
typedef uint32_t PbMessageKind;

#define PB_LOGTYPE_CONSOLE ((PbLogType)0u)
#define PB_LOGTYPE_LOGFILE ((PbLogType)1u)
#define PB_LOGTYPE_CONSOLE_AND_LOGFILE ((PbLogType)2u)
#define PB_MESSAGE_KIND_ASSERT ((PbMessageKind)0u)
#define PB_MESSAGE_KIND_CONSOLE ((PbMessageKind)1u)
#define PB_MESSAGE_KIND_CONSOLE_NO_PREFIX ((PbMessageKind)2u)
#define PB_MESSAGE_KIND_CRITICAL_ERROR ((PbMessageKind)3u)
#define PB_MESSAGE_KIND_DEBUG ((PbMessageKind)4u)
#define PB_MESSAGE_KIND_ERROR ((PbMessageKind)5u)
#define PB_MESSAGE_KIND_INFO ((PbMessageKind)6u)
#define PB_MESSAGE_KIND_KNOWN ((PbMessageKind)7u)
#define PB_MESSAGE_KIND_LOG ((PbMessageKind)8u)
#define PB_MESSAGE_KIND_NONFATAL_ERROR ((PbMessageKind)9u)
#define PB_MESSAGE_KIND_OPPORTUNITY ((PbMessageKind)10u)
#define PB_MESSAGE_KIND_PHASE ((PbMessageKind)11u)
#define PB_MESSAGE_KIND_STATS ((PbMessageKind)12u)
#define PB_MESSAGE_KIND_WARNING ((PbMessageKind)13u)
#define PB_TLS_KEY_INTERNAL_EXCEPTION ((PbTlsKey)0)
#define PB_TLS_KEY_CLIENT_FIRST ((PbTlsKey)1)
#define PB_TLS_KEY_CLIENT_LAST ((PbTlsKey)64)
#define PB_MAX_CLIENT_TLS_KEYS UINT32_C(64)
#define PB_INVALID_OS_THREAD_ID UINT32_MAX
#define PB_INVALID_PIN_THREAD_UID UINT64_MAX
#define PB_INVALID_THREAD_ID UINT32_MAX
#define PB_INVALID_TLS_KEY ((PbTlsKey)-1)

#define PB_PROBE_MODE_DEFAULT ((PbProbeMode)0u)
#define PB_PROBE_MODE_ALLOW_RELOCATION ((PbProbeMode)1u)
#define PB_PROBE_MODE_ALLOW_POTENTIAL_BRANCH_TARGET ((PbProbeMode)2u)

#define PB_MEMORY_TYPE_READ ((PbMemoryType)0u)
#define PB_MEMORY_TYPE_WRITE ((PbMemoryType)1u)
#define PB_MEMORY_TYPE_READ2 ((PbMemoryType)2u)

#define PB_PREDICATE_ALWAYS_TRUE ((PbPredicate)0u)
#define PB_PREDICATE_INVALID ((PbPredicate)1u)
#define PB_PREDICATE_BELOW ((PbPredicate)2u)
#define PB_PREDICATE_BELOW_OR_EQUAL ((PbPredicate)3u)
#define PB_PREDICATE_LESS ((PbPredicate)4u)
#define PB_PREDICATE_LESS_OR_EQUAL ((PbPredicate)5u)
#define PB_PREDICATE_NOT_BELOW ((PbPredicate)6u)
#define PB_PREDICATE_NOT_BELOW_OR_EQUAL ((PbPredicate)7u)
#define PB_PREDICATE_NOT_LESS ((PbPredicate)8u)
#define PB_PREDICATE_NOT_LESS_OR_EQUAL ((PbPredicate)9u)
#define PB_PREDICATE_NOT_OVERFLOW ((PbPredicate)10u)
#define PB_PREDICATE_NOT_PARITY ((PbPredicate)11u)
#define PB_PREDICATE_NOT_SIGN ((PbPredicate)12u)
#define PB_PREDICATE_NOT_ZERO ((PbPredicate)13u)
#define PB_PREDICATE_OVERFLOW ((PbPredicate)14u)
#define PB_PREDICATE_PARITY ((PbPredicate)15u)
#define PB_PREDICATE_SIGN ((PbPredicate)16u)
#define PB_PREDICATE_ZERO ((PbPredicate)17u)
#define PB_PREDICATE_CX_NON_ZERO ((PbPredicate)18u)
#define PB_PREDICATE_ECX_NON_ZERO ((PbPredicate)19u)
#define PB_PREDICATE_RCX_NON_ZERO ((PbPredicate)20u)
#define PB_PREDICATE_SAVED_GCX_NON_ZERO ((PbPredicate)21u)
#define PB_PREDICATE_LAST ((PbPredicate)22u)

#define PB_VSYSCALL_NR UINT32_C(0xABCDDCBA)

/* Matched Pin SDK instrumentation argument identities. */
#include "pinbridge/generated/inst_args.inc"

#define PB_MAX_MULTI_MEMOPS 16u
#define PB_FAST_ANALYSIS_CALL
#define PB_IARG_LIST_MAX_DESCRIPTORS 32u

typedef struct PbIargListOpaque* PbIargListHandle;
typedef struct PbKnobOpaque* PbKnobHandle;
typedef uint32_t PbKnobMode;
#define PB_KNOB_HANDLE_INVALID ((PbKnobHandle)0)
#define PB_KNOB_MODE_INVALID ((PbKnobMode)0u)
#define PB_KNOB_MODE_COMMENT ((PbKnobMode)1u)
#define PB_KNOB_MODE_WRITEONCE ((PbKnobMode)2u)
#define PB_KNOB_MODE_OVERWRITE ((PbKnobMode)3u)
#define PB_KNOB_MODE_ACCUMULATE ((PbKnobMode)4u)
#define PB_KNOB_MODE_APPEND ((PbKnobMode)5u)
#define PB_KNOB_MODE_LAST ((PbKnobMode)6u)

typedef uint32_t PbDebuggerType;
typedef uint32_t PbDebuggingEvent;
typedef uint32_t PbDebugConnectionType;
typedef uint32_t PbDebugModeOption;
typedef uint32_t PbDebugStatus;
typedef uint32_t PbExceptionClass;
typedef uint32_t PbExceptionCode;
typedef uint32_t PbFaultyAccessType;
typedef uint32_t PbFpError;

#define PB_DEBUGGER_TYPE_UNKNOWN ((PbDebuggerType)0u)
#define PB_DEBUGGER_TYPE_GDB ((PbDebuggerType)1u)
#define PB_DEBUGGER_TYPE_LLDB ((PbDebuggerType)2u)
#define PB_DEBUGGER_TYPE_IDB ((PbDebuggerType)3u)
#define PB_DEBUGGER_TYPE_VISUAL_STUDIO_VSDBG ((PbDebuggerType)4u)
#define PB_DEBUGGER_TYPE_VISUAL_STUDIO ((PbDebuggerType)5u)
#define PB_DEBUGGING_EVENT_BREAKPOINT ((PbDebuggingEvent)0u)
#define PB_DEBUGGING_EVENT_SINGLE_STEP ((PbDebuggingEvent)1u)
#define PB_DEBUGGING_EVENT_ASYNC_BREAK ((PbDebuggingEvent)2u)
#define PB_DEBUG_CONNECTION_TYPE_NONE ((PbDebugConnectionType)0u)
#define PB_DEBUG_CONNECTION_TYPE_TCP_SERVER ((PbDebugConnectionType)1u)
#define PB_DEBUG_CONNECTION_TYPE_TCP_CLIENT ((PbDebugConnectionType)2u)
#define PB_DEBUG_MODE_OPTION_NONE ((PbDebugModeOption)0u)
#define PB_DEBUG_MODE_OPTION_STOP_AT_ENTRY ((PbDebugModeOption)1u)
#define PB_DEBUG_MODE_OPTION_SILENT ((PbDebugModeOption)2u)
#define PB_DEBUG_MODE_OPTION_ALLOW_REMOTE ((PbDebugModeOption)4u)
#define PB_DEBUG_STATUS_DISABLED ((PbDebugStatus)0u)
#define PB_DEBUG_STATUS_UNCONNECTABLE ((PbDebugStatus)1u)
#define PB_DEBUG_STATUS_UNCONNECTED ((PbDebugStatus)2u)
#define PB_DEBUG_STATUS_CONNECTED ((PbDebugStatus)3u)
#define PB_EXCEPTCLASS_NONE ((PbExceptionClass)0u)
#define PB_EXCEPTCLASS_UNKNOWN ((PbExceptionClass)1u)
#define PB_EXCEPTCLASS_ACCESS_FAULT ((PbExceptionClass)2u)
#define PB_EXCEPTCLASS_INVALID_INS ((PbExceptionClass)3u)
#define PB_EXCEPTCLASS_INT_ERROR ((PbExceptionClass)4u)
#define PB_EXCEPTCLASS_FP_ERROR ((PbExceptionClass)5u)
#define PB_EXCEPTCLASS_MULTIPLE_FP_ERROR ((PbExceptionClass)6u)
#define PB_EXCEPTCLASS_DEBUG ((PbExceptionClass)7u)
#define PB_EXCEPTCLASS_OS ((PbExceptionClass)8u)
#define PB_EXCEPTCODE_NONE ((PbExceptionCode)0u)
#define PB_EXCEPTCODE_ACCESS_INVALID_ADDRESS ((PbExceptionCode)1u)
#define PB_EXCEPTCODE_ACCESS_DENIED ((PbExceptionCode)2u)
#define PB_EXCEPTCODE_ACCESS_INVALID_PAGE ((PbExceptionCode)3u)
#define PB_EXCEPTCODE_ACCESS_MISALIGNED ((PbExceptionCode)4u)
#define PB_EXCEPTCODE_ILLEGAL_INS ((PbExceptionCode)5u)
#define PB_EXCEPTCODE_PRIVILEGED_INS ((PbExceptionCode)6u)
#define PB_EXCEPTCODE_INT_DIVIDE_BY_ZERO ((PbExceptionCode)7u)
#define PB_EXCEPTCODE_INT_OVERFLOW_TRAP ((PbExceptionCode)8u)
#define PB_EXCEPTCODE_INT_BOUNDS_EXCEEDED ((PbExceptionCode)9u)
#define PB_EXCEPTCODE_X87_DIVIDE_BY_ZERO ((PbExceptionCode)10u)
#define PB_EXCEPTCODE_X87_OVERFLOW ((PbExceptionCode)11u)
#define PB_EXCEPTCODE_X87_UNDERFLOW ((PbExceptionCode)12u)
#define PB_EXCEPTCODE_X87_INEXACT_RESULT ((PbExceptionCode)13u)
#define PB_EXCEPTCODE_X87_INVALID_OPERATION ((PbExceptionCode)14u)
#define PB_EXCEPTCODE_X87_DENORMAL_OPERAND ((PbExceptionCode)15u)
#define PB_EXCEPTCODE_X87_STACK_ERROR ((PbExceptionCode)16u)
#define PB_EXCEPTCODE_SIMD_DIVIDE_BY_ZERO ((PbExceptionCode)17u)
#define PB_EXCEPTCODE_SIMD_OVERFLOW ((PbExceptionCode)18u)
#define PB_EXCEPTCODE_SIMD_UNDERFLOW ((PbExceptionCode)19u)
#define PB_EXCEPTCODE_SIMD_INEXACT_RESULT ((PbExceptionCode)20u)
#define PB_EXCEPTCODE_SIMD_INVALID_OPERATION ((PbExceptionCode)21u)
#define PB_EXCEPTCODE_SIMD_DENORMAL_OPERAND ((PbExceptionCode)22u)
#define PB_EXCEPTCODE_DBG_BREAKPOINT_TRAP ((PbExceptionCode)23u)
#define PB_EXCEPTCODE_DBG_SINGLE_STEP_TRAP ((PbExceptionCode)24u)
#define PB_EXCEPTCODE_ACCESS_WINDOWS_GUARD_PAGE ((PbExceptionCode)25u)
#define PB_EXCEPTCODE_ACCESS_WINDOWS_STACK_OVERFLOW ((PbExceptionCode)26u)
#define PB_EXCEPTCODE_WINDOWS ((PbExceptionCode)27u)
#define PB_EXCEPTCODE_RECEIVED_UNKNOWN ((PbExceptionCode)28u)
#define PB_EXCEPTCODE_RECEIVED_ACCESS_FAULT ((PbExceptionCode)29u)
#define PB_EXCEPTCODE_RECEIVED_AMBIGUOUS_X87 ((PbExceptionCode)30u)
#define PB_EXCEPTCODE_RECEIVED_AMBIGUOUS_SIMD ((PbExceptionCode)31u)
#define PB_FAULTY_ACCESS_TYPE_UNKNOWN ((PbFaultyAccessType)0u)
#define PB_FAULTY_ACCESS_READ ((PbFaultyAccessType)1u)
#define PB_FAULTY_ACCESS_WRITE ((PbFaultyAccessType)2u)
#define PB_FAULTY_ACCESS_EXECUTE ((PbFaultyAccessType)3u)
#define PB_FPERROR_DIVIDE_BY_ZERO ((PbFpError)1u)
#define PB_FPERROR_OVERFLOW ((PbFpError)2u)
#define PB_FPERROR_UNDERFLOW ((PbFpError)4u)
#define PB_FPERROR_INEXACT_RESULT ((PbFpError)8u)
#define PB_FPERROR_INVALID_OPERATION ((PbFpError)16u)
#define PB_FPERROR_DENORMAL_OPERAND ((PbFpError)32u)
#define PB_FPERROR_X87_STACK_ERROR ((PbFpError)64u)
#define PB_MAX_WINDOWS_EXCEPTION_ARGS 5u

typedef struct PbDebuggerRegDescription
{
    PbRegId pin_reg;
    uint32_t tool_reg_id;
    uint32_t width_in_bits;
    const char* name;
    int32_t gcc_id;
} PbDebuggerRegDescription;

typedef struct PbDebugConnectionInfo
{
    PbDebugConnectionType type;
    uint8_t stop_at_entry;
    uint8_t reserved[3];
    int32_t tcp_port;
} PbDebugConnectionInfo;

typedef struct PbDebugMode
{
    PbDebugConnectionType type;
    PbDebugModeOption options;
    const char* tcp_client_ip;
    int32_t tcp_port;
    uint32_t reserved;
} PbDebugMode;

typedef uint8_t (PB_CALL *PbDebugBreakpointCallback)(
    uint64_t address, uint32_t size, uint8_t insert, void* user_data);
typedef uint8_t (PB_CALL *PbDebugInterpreterCallback)(
    PbThreadId thread_id, PbContextHandle context, const char* command,
    const char** reply_utf8, void* user_data);
typedef void (PB_CALL *PbGetEmulatedRegisterCallback)(
    uint32_t tool_reg_id, PbThreadId thread_id, PbContextHandle context,
    void* data, void* user_data);
typedef uint64_t (PB_CALL *PbGetTargetDescriptionCallback)(
    const char* name, uint64_t size, void* buffer, void* user_data);
typedef uint8_t (PB_CALL *PbInterceptDebuggingEventCallback)(
    PbThreadId thread_id, PbDebuggingEvent event_type,
    PbContextHandle context, void* user_data);
typedef void (PB_CALL *PbSetEmulatedRegisterCallback)(
    uint32_t tool_reg_id, PbThreadId thread_id, PbContextHandle context,
    const void* data, void* user_data);
typedef uint8_t (PB_CALL *PbMessageCallback)(
    const char* message, PbPinErrorType type, int32_t user_type,
    int32_t severity, const uint64_t* arguments, uint32_t argument_count,
    void* user_data);
typedef void (PB_CALL *PbTlsDestructor)(void* data);
typedef void (PB_CALL *PbThreadRootCallback)(void* argument);

typedef struct PbIargDescriptor
{
    PbIargType type;
    uint32_t reserved;
    uint64_t value;
    uint64_t value2;
} PbIargDescriptor;

#define PB_IARG_LIST_INVALID ((PbIargListHandle)0)

typedef struct PbProtoOpaque* PbProtoHandle;

typedef struct PbProtoArg
{
    PbProtoArgKind kind;
    uint32_t reserved;
    uint64_t size;
} PbProtoArg;

#define PB_PROTO_HANDLE_INVALID ((PbProtoHandle)0)
#define PB_PROTO_MAX_ARGUMENTS 8u

#define PB_CALLINGSTD_INVALID ((PbCallingStandard)0u)
#define PB_CALLINGSTD_DEFAULT ((PbCallingStandard)1u)
#define PB_CALLINGSTD_CDECL ((PbCallingStandard)2u)
#define PB_CALLINGSTD_REGPARMS ((PbCallingStandard)3u)
#define PB_CALLINGSTD_STDCALL ((PbCallingStandard)4u)
#define PB_CALLINGSTD_ART ((PbCallingStandard)5u)

#define PB_PARG_INVALID ((PbProtoArgKind)0u)
#define PB_PARG_POINTER ((PbProtoArgKind)1u)
#define PB_PARG_BOOL ((PbProtoArgKind)2u)
#define PB_PARG_CHAR ((PbProtoArgKind)3u)
#define PB_PARG_UCHAR ((PbProtoArgKind)4u)
#define PB_PARG_SCHAR ((PbProtoArgKind)5u)
#define PB_PARG_SHORT ((PbProtoArgKind)6u)
#define PB_PARG_USHORT ((PbProtoArgKind)7u)
#define PB_PARG_INT ((PbProtoArgKind)8u)
#define PB_PARG_UINT ((PbProtoArgKind)9u)
#define PB_PARG_LONG ((PbProtoArgKind)10u)
#define PB_PARG_ULONG ((PbProtoArgKind)11u)
#define PB_PARG_LONGLONG ((PbProtoArgKind)12u)
#define PB_PARG_ULONGLONG ((PbProtoArgKind)13u)
#define PB_PARG_FLOAT ((PbProtoArgKind)14u)
#define PB_PARG_DOUBLE ((PbProtoArgKind)15u)
#define PB_PARG_VOID ((PbProtoArgKind)16u)
#define PB_PARG_ENUM ((PbProtoArgKind)17u)
#define PB_PARG_AGGREGATE ((PbProtoArgKind)18u)
#define PB_PARG_END ((PbProtoArgKind)19u)

#define PB_UNDECORATION_COMPLETE ((PbUndecoration)0u)
#define PB_UNDECORATION_NAME_ONLY ((PbUndecoration)1u)

#define PB_BUFFER_ID_INVALID ((PbBufferId)0u)

#define PB_IMG_PROPERTY_INVALID ((PbImgProperty)0u)
#define PB_IMG_PROPERTY_SHSTK_ENABLED ((PbImgProperty)1u)
#define PB_IMG_PROPERTY_IBT_ENABLED ((PbImgProperty)2u)
#define PB_IMG_PROPERTY_LAST ((PbImgProperty)3u)

#define PB_IMG_TYPE_INVALID ((PbImgType)0u)
#define PB_IMG_TYPE_STATIC ((PbImgType)1u)
#define PB_IMG_TYPE_SHARED ((PbImgType)2u)
#define PB_IMG_TYPE_SHAREDLIB ((PbImgType)3u)
#define PB_IMG_TYPE_RELOCATABLE ((PbImgType)4u)
#define PB_IMG_TYPE_DYNAMIC_CODE ((PbImgType)5u)
#define PB_IMG_TYPE_API_CREATED ((PbImgType)6u)
#define PB_IMG_TYPE_LAST ((PbImgType)7u)

#define PB_SYSCALL_STANDARD_INVALID ((PbSyscallStandard)0u)
#define PB_SYSCALL_STANDARD_IA32_LINUX ((PbSyscallStandard)1u)
#define PB_SYSCALL_STANDARD_IA32_LINUX_SYSENTER ((PbSyscallStandard)2u)
#define PB_SYSCALL_STANDARD_IA32E_LINUX ((PbSyscallStandard)3u)
#define PB_SYSCALL_STANDARD_IA32E_LINUX_VSYSCALL ((PbSyscallStandard)4u)
#define PB_SYSCALL_STANDARD_IA32_MAC ((PbSyscallStandard)5u)
#define PB_SYSCALL_STANDARD_IA32E_MAC ((PbSyscallStandard)6u)
#define PB_SYSCALL_STANDARD_IA32_WINDOWS_FAST ((PbSyscallStandard)7u)
#define PB_SYSCALL_STANDARD_IA32E_WINDOWS_FAST ((PbSyscallStandard)8u)
#define PB_SYSCALL_STANDARD_IA32_WINDOWS_ALT ((PbSyscallStandard)9u)
#define PB_SYSCALL_STANDARD_WOW64 ((PbSyscallStandard)10u)
#define PB_SYSCALL_STANDARD_WINDOWS_INT ((PbSyscallStandard)11u)

#include "pinbridge/generated/reg_ids.inc"
#include "pinbridge/generated/reg_metadata.inc"
#include "pinbridge/generated/processor_state.inc"
#include "pinbridge/generated/sec_types.inc"
#include "pinbridge/generated/control_enums.inc"

#define PB_CALL_ORDER_FIRST ((PbCallOrder)100)
#define PB_CALL_ORDER_DEFAULT ((PbCallOrder)200)
#define PB_CALL_ORDER_LAST ((PbCallOrder)300)
#include "pinbridge/generated/error_file_enums.inc"

#define PB_PIN_ERROR_ARGUMENT_LIMIT 8u

/* ABI-owned register bitmap. Bit N represents PbRegId N. The maximum is a
   storage limit, not proof that a value is a valid REG in the matched SDK. */
#define PB_REGSET_WORD_COUNT 16u
#define PB_REGSET_MAX_REG_ID ((PB_REGSET_WORD_COUNT * 64u) - 1u)
typedef struct PbRegSet
{
    uint64_t words[PB_REGSET_WORD_COUNT];
} PbRegSet;

/* ABI-owned opaque bytes for the Intel64 promoted FXSAVE memory format. */
#define PB_FXSAVE_SIZE 512u
typedef struct PbFxSave
{
    uint8_t bytes[PB_FXSAVE_SIZE];
} PbFxSave;

/* ABI-owned snapshot passed by const pointer during memory translation callbacks. */
typedef struct PbMemoryTransInfo
{
    uint64_t address;
    uint64_t size;
    uint64_t instruction_pointer;
    PbThreadId thread_id;
    uint32_t memory_operation;
    uint8_t is_atomic;
    uint8_t is_rmw;
    uint8_t is_prefetch;
    uint8_t is_from_pin;
    uint32_t reserved;
} PbMemoryTransInfo;

/* ABI-owned value snapshot of a half-open memory range [base, base + size). */
typedef struct PbMemRange
{
    uint64_t base;
    uint64_t size;
} PbMemRange;

#define PB_EXCEPTION_INFO_HAS_FAULT_ADDRESS UINT32_C(1)
#define PB_EXCEPTION_INFO_HAS_FP_ERRORS UINT32_C(2)
#define PB_EXCEPTION_INFO_HAS_WINDOWS_DETAILS UINT32_C(4)

/* ABI-owned value snapshot of the public EXCEPTION_INFO query surface. */
typedef struct PbExceptionInfoSnapshot
{
    uint32_t exception_code;
    uint32_t exception_class;
    uint64_t exception_address;
    uint32_t flags;
    uint32_t faulty_access_type;
    uint64_t faulty_access_address;
    uint32_t fp_errors;
    uint32_t windows_exception_code;
    uint32_t windows_argument_count;
    uint32_t reserved;
    uint64_t windows_arguments[5];
} PbExceptionInfoSnapshot;

typedef void(PB_CALL* PbInsInstrumentCallback)(PbInsHandle ins, void* user_data);
typedef void(PB_CALL* PbInsAnalysisCallback)(void* user_data);
/* Analysis callback that also receives the thread context (IARG_CONTEXT).
   Legal uses include reading registers and redirecting execution via
   pb_pin_set_context_reg + pb_pin_execute_at (ABI v1.3). */
typedef void(PB_CALL* PbInsContextAnalysisCallback)(
    PbContextHandle context, void* user_data);
typedef uint64_t(PB_CALL* PbInsPredicateCallback)(void* user_data);
typedef void(PB_CALL* PbTraceInstrumentCallback)(PbTraceHandle trace, void* user_data);
typedef void(PB_CALL* PbRtnInstrumentCallback)(PbRtnHandle rtn, void* user_data);
typedef void(PB_CALL* PbImgInstrumentCallback)(PbImgHandle img, void* user_data);
typedef void(PB_CALL* PbApplicationStartCallback)(void* user_data);
typedef void(PB_CALL* PbPrepareForFiniCallback)(void* user_data);
typedef void(PB_CALL* PbFiniCallback)(int32_t code, void* user_data);
typedef void(PB_CALL* PbDetachCallback)(void* user_data);
typedef void(PB_CALL* PbDetachProbedCallback)(void* user_data);
typedef void(PB_CALL* PbAttachProbedCallback)(void* user_data);
typedef void(PB_CALL* PbOutOfMemoryCallback)(
    uint64_t requested_size, void* user_data);
typedef void(PB_CALL* PbThreadStartCallback)(
    PbThreadId thread_id, PbContextHandle context, int32_t flags, void* user_data);
typedef void(PB_CALL* PbThreadFiniCallback)(
    PbThreadId thread_id, PbConstContextHandle context, int32_t code, void* user_data);
/* from/to are borrowed for the callback duration; either may be NULL per Pin's reason rules. */
typedef void(PB_CALL* PbContextChangeCallback)(
    PbThreadId thread_id, PbContextChangeReason reason,
    PbConstContextHandle from, PbContextHandle to, int32_t info, void* user_data);
typedef void(PB_CALL* PbXedDecodeCallback)(
    PbXedDecodedInstHandle decoded_instruction, void* user_data);
typedef uint64_t(PB_CALL* PbFetchCallback)(
    void* buffer, uint64_t address, uint64_t size,
    PbExceptionInfoHandle exception_info, void* user_data);
typedef PbExceptHandlingResult(PB_CALL* PbInternalExceptionCallback)(
    PbThreadId thread_id, PbExceptionInfoHandle exception_info,
    PbPhysicalContextHandle physical_context, void* user_data);
typedef uint64_t(PB_CALL* PbMemoryAddressTransCallback)(
    const PbMemoryTransInfo* info, void* user_data);
/* Return zero to leave the child uninstrumented, nonzero to follow it. */
typedef uint8_t(PB_CALL* PbFollowChildProcessCallback)(
    PbChildProcessHandle child, void* user_data);
typedef void(PB_CALL* PbProbedCallCallback)(void* user_data);
typedef void(PB_CALL* PbBblAnalysisCallback)(void* user_data);
typedef uint64_t(PB_CALL* PbBblPredicateCallback)(void* user_data);
typedef void(PB_CALL* PbTraceAnalysisCallback)(void* user_data);
typedef uint64_t(PB_CALL* PbTracePredicateCallback)(void* user_data);
typedef void(PB_CALL* PbTraceSmcCallback)(
    uint64_t trace_start, uint64_t trace_end, void* user_data);
typedef void*(PB_CALL* PbTraceBufferCallback)(
    PbBufferId id, PbThreadId thread_id, PbConstContextHandle context,
    void* buffer, uint64_t num_elements, void* user_data);
typedef void(PB_CALL* PbSyscallEntryCallback)(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard, void* user_data);
typedef void(PB_CALL* PbSyscallExitCallback)(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard, void* user_data);

PB_API uint32_t PB_CALL pb_abi_version(void);

/* Fixed-width Pin UTILS value and conversion core. String inputs are NUL-
   terminated; pointer outputs are borrowed values and are never owned here. */
PB_API PbStatus PB_CALL pb_addrint_to_pointer(
    uint64_t address, void** out_pointer);
PB_API PbStatus PB_CALL pb_addrint_from_string(
    const char* text, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_bit_count(uint64_t value, uint32_t* out_count);
PB_API PbStatus PB_CALL pb_char_is_space(char value, uint8_t* out_is_space);
PB_API PbStatus PB_CALL pb_char_to_hex_digit(char value, int32_t* out_digit);
PB_API PbStatus PB_CALL pb_char_to_upper(char value, char* out_upper);
PB_API PbStatus PB_CALL pb_flt64_from_string(
    const char* text, double* out_value);
PB_API PbStatus PB_CALL pb_get_page_of_addr(
    uint64_t address, uint64_t* out_page);
PB_API PbStatus PB_CALL pb_mem_page_range_addr(
    uint64_t address, PbMemRange* out_range);
PB_API PbStatus PB_CALL pb_mem_page_range_pointer(
    const void* pointer, PbMemRange* out_range);
PB_API PbStatus PB_CALL pb_get_sp(const void** out_stack_pointer);
PB_API PbStatus PB_CALL pb_int32_from_string(
    const char* text, int32_t* out_value);
PB_API PbStatus PB_CALL pb_int64_from_string(
    const char* text, int64_t* out_value);
PB_API PbStatus PB_CALL pb_ptr_at_offset(
    void* pointer, uint64_t offset, void** out_pointer);
PB_API PbStatus PB_CALL pb_const_ptr_at_offset(
    const void* pointer, uint64_t offset, const void** out_pointer);
PB_API PbStatus PB_CALL pb_ptr_at_offset_typed(
    void* pointer, uint64_t offset, void** out_pointer);
PB_API PbStatus PB_CALL pb_const_ptr_at_offset_typed(
    const void* pointer, uint64_t offset, const void** out_pointer);
PB_API PbStatus PB_CALL pb_read_line(
    const char* input, uint64_t input_size, uint64_t offset,
    uint32_t line_number, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint64_t* out_next_offset,
    uint32_t* out_line_number);
PB_API PbStatus PB_CALL pb_knob_check_all(uint8_t allow_dashes);
PB_API PbStatus PB_CALL pb_knob_compare(
    PbKnobHandle left, PbKnobHandle right, int32_t* out_result);
PB_API PbStatus PB_CALL pb_knob_find_enabled(
    const char* name, PbKnobHandle* out_knob);
PB_API PbStatus PB_CALL pb_knob_find_family(
    const char* family, PbKnobHandle* out_knob);
PB_API PbStatus PB_CALL pb_knob_find(
    const char* name, PbKnobHandle* out_knob);
PB_API PbStatus PB_CALL pb_knob_slow_asserts(uint8_t* out_enabled);
PB_API PbStatus PB_CALL pb_knob_count(uint32_t* out_count);
PB_API PbStatus PB_CALL pb_knob_set_by_user(
    PbKnobHandle knob, uint8_t* out_set_by_user);
PB_API PbStatus PB_CALL pb_knob_summary(
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_knob_long_all(
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_knob_turn_on_set_by_user(PbKnobHandle knob);
PB_API PbStatus PB_CALL pb_img_get_loader_info(
    PbImgHandle image, uint64_t* out_loader_info);
PB_API PbStatus PB_CALL pb_img_set_loader_info(
    PbImgHandle image, uint64_t loader_info);
PB_API PbStatus PB_CALL pb_pin_add_breakpoint_handler(
    PbDebugBreakpointCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_debug_interpreter(
    PbDebugInterpreterCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_debugger_register_emulator(
    const PbDebuggerRegDescription* registers, uint32_t register_count,
    PbGetEmulatedRegisterCallback get_callback,
    PbSetEmulatedRegisterCallback set_callback,
    PbGetTargetDescriptionCallback description_callback, void* user_data);
PB_API PbStatus PB_CALL pb_pin_application_breakpoint(
    PbConstContextHandle context, PbThreadId thread_id,
    uint8_t wait_if_no_debugger, const char* message);
PB_API PbStatus PB_CALL pb_pin_change_pending_tool_breakpoint(
    PbThreadId thread_id, uint8_t squash, const char* message,
    uint8_t* out_changed);
PB_API PbStatus PB_CALL pb_pin_get_debug_connection_info(
    PbDebugConnectionInfo* out_info, uint8_t* out_enabled);
PB_API PbStatus PB_CALL pb_pin_get_debug_status(PbDebugStatus* out_status);
PB_API PbStatus PB_CALL pb_pin_get_debugger_type(PbDebuggerType* out_type);
PB_API PbStatus PB_CALL pb_pin_get_pending_tool_breakpoint(
    PbThreadId thread_id, char* buffer, uint64_t capacity,
    uint64_t* required_size, uint8_t* out_pending);
PB_API PbStatus PB_CALL pb_pin_intercept_debugging_event(
    PbDebuggingEvent event_type,
    PbInterceptDebuggingEventCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_pin_remove_breakpoint_handler(
    PbDebugBreakpointCallback callback);
PB_API PbStatus PB_CALL pb_pin_remove_debug_interpreter(
    PbDebugInterpreterCallback callback);
PB_API PbStatus PB_CALL pb_pin_reset_breakpoint_at(uint64_t address);
PB_API PbStatus PB_CALL pb_pin_set_debug_mode(
    const PbDebugMode* mode, uint8_t* out_accepted);
PB_API PbStatus PB_CALL pb_pin_wait_for_debugger(
    uint32_t timeout_ms, uint8_t* out_connected);
PB_API PbStatus PB_CALL pb_exception_info_release(PbExceptionInfoHandle info);
PB_API PbStatus PB_CALL pb_pin_count_windows_exception_arguments(
    PbExceptionInfoHandle info, uint32_t* out_count);
PB_API PbStatus PB_CALL pb_pin_exception_to_string(
    PbExceptionInfoHandle info, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_pin_get_exception_address(
    PbExceptionInfoHandle info, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_pin_get_exception_class(
    PbExceptionCode code, PbExceptionClass* out_class);
PB_API PbStatus PB_CALL pb_pin_get_exception_code(
    PbExceptionInfoHandle info, PbExceptionCode* out_code);
PB_API PbStatus PB_CALL pb_pin_get_faulty_access_address(
    PbExceptionInfoHandle info, uint64_t* out_address, uint8_t* out_known);
PB_API PbStatus PB_CALL pb_pin_get_faulty_access_type(
    PbExceptionInfoHandle info, PbFaultyAccessType* out_type);
PB_API PbStatus PB_CALL pb_pin_get_fp_error_set(
    PbExceptionInfoHandle info, uint32_t* out_errors);
PB_API PbStatus PB_CALL pb_pin_get_windows_exception_argument(
    PbExceptionInfoHandle info, uint32_t index, uint64_t* out_argument);
PB_API PbStatus PB_CALL pb_pin_get_windows_exception_code(
    PbExceptionInfoHandle info, uint32_t* out_code);
PB_API PbStatus PB_CALL pb_pin_init_access_fault_info(
    PbExceptionCode code, uint64_t exception_address,
    uint64_t access_address, PbFaultyAccessType access_type,
    PbExceptionInfoHandle* out_info);
PB_API PbStatus PB_CALL pb_pin_init_exception_info(
    PbExceptionCode code, uint64_t exception_address,
    PbExceptionInfoHandle* out_info);
PB_API PbStatus PB_CALL pb_pin_init_windows_exception_info(
    uint32_t system_code, uint64_t exception_address,
    const uint64_t* arguments, uint32_t argument_count,
    PbExceptionInfoHandle* out_info);
PB_API PbStatus PB_CALL pb_pin_raise_exception(
    PbConstContextHandle context, PbThreadId thread_id,
    PbExceptionInfoHandle info);
PB_API PbStatus PB_CALL pb_pin_set_exception_address(
    PbExceptionInfoHandle info, uint64_t address);
PB_API PbStatus PB_CALL pb_pin_init_lock(PbLockHandle* out_lock);
PB_API PbStatus PB_CALL pb_pin_lock_destroy(PbLockHandle lock);
PB_API PbStatus PB_CALL pb_pin_get_lock(PbLockHandle lock, int32_t value);
PB_API PbStatus PB_CALL pb_pin_release_lock(
    PbLockHandle lock, int32_t* out_owner);
PB_API PbStatus PB_CALL pb_pin_mutex_init(PbMutexHandle* out_mutex);
PB_API PbStatus PB_CALL pb_pin_mutex_fini(PbMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_mutex_lock(PbMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_mutex_try_lock(
    PbMutexHandle mutex, uint8_t* out_acquired);
PB_API PbStatus PB_CALL pb_pin_mutex_unlock(PbMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_rwmutex_init(PbRwMutexHandle* out_mutex);
PB_API PbStatus PB_CALL pb_pin_rwmutex_fini(PbRwMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_rwmutex_read_lock(PbRwMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_rwmutex_try_read_lock(
    PbRwMutexHandle mutex, uint8_t* out_acquired);
PB_API PbStatus PB_CALL pb_pin_rwmutex_try_write_lock(
    PbRwMutexHandle mutex, uint8_t* out_acquired);
PB_API PbStatus PB_CALL pb_pin_rwmutex_unlock(PbRwMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_rwmutex_write_lock(PbRwMutexHandle mutex);
PB_API PbStatus PB_CALL pb_pin_semaphore_init(
    PbSemaphoreHandle* out_semaphore);
PB_API PbStatus PB_CALL pb_pin_semaphore_fini(PbSemaphoreHandle semaphore);
PB_API PbStatus PB_CALL pb_pin_semaphore_clear(PbSemaphoreHandle semaphore);
PB_API PbStatus PB_CALL pb_pin_semaphore_is_set(
    PbSemaphoreHandle semaphore, uint8_t* out_is_set);
PB_API PbStatus PB_CALL pb_pin_semaphore_set(PbSemaphoreHandle semaphore);
PB_API PbStatus PB_CALL pb_pin_semaphore_timed_wait(
    PbSemaphoreHandle semaphore, uint32_t timeout_ms, uint8_t* out_is_set);
PB_API PbStatus PB_CALL pb_pin_semaphore_wait(PbSemaphoreHandle semaphore);
PB_API PbStatus PB_CALL pb_assert_string(
    const char* file_name, const char* function_name, uint32_t line,
    const char* message, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_break_me(void);
PB_API PbStatus PB_CALL pb_milliseconds_elapsed(uint64_t* out_milliseconds);
PB_API PbStatus PB_CALL pb_pin_create_thread_data_key(
    PbTlsDestructor destructor, PbTlsKey* out_key);
PB_API PbStatus PB_CALL pb_pin_delete_thread_data_key(
    PbTlsKey key, uint8_t* out_deleted);
PB_API PB_NORETURN void PB_CALL pb_pin_exit_thread(int32_t exit_code);
PB_API PbStatus PB_CALL pb_pin_get_parent_tid(PbOsThreadId* out_thread_id);
PB_API PbStatus PB_CALL pb_pin_get_stopped_thread_context(
    PbThreadId thread_id, PbConstContextHandle* out_context);
PB_API PbStatus PB_CALL pb_pin_get_stopped_thread_count(uint32_t* out_count);
PB_API PbStatus PB_CALL pb_pin_get_stopped_thread_id(
    uint32_t index, PbThreadId* out_thread_id);
PB_API PbStatus PB_CALL pb_pin_get_stopped_thread_writeable_context(
    PbThreadId thread_id, PbContextHandle* out_context);
PB_API PbStatus PB_CALL pb_pin_get_thread_data(
    PbTlsKey key, PbThreadId thread_id, void** out_data);
PB_API PbStatus PB_CALL pb_pin_get_tid(PbOsThreadId* out_thread_id);
PB_API PbStatus PB_CALL pb_pin_is_application_thread(uint8_t* out_is_application);
PB_API PbStatus PB_CALL pb_pin_is_thread_stopped_in_debugger(
    PbThreadId thread_id, uint8_t* out_is_stopped);
PB_API PbStatus PB_CALL pb_pin_resume_application_threads(PbThreadId thread_id);
PB_API PbStatus PB_CALL pb_pin_set_thread_data(
    PbTlsKey key, const void* data, PbThreadId thread_id, uint8_t* out_set);
PB_API PbStatus PB_CALL pb_pin_sleep(uint32_t milliseconds);
PB_API PbStatus PB_CALL pb_pin_spawn_application_thread(
    PbConstContextHandle context, uint8_t* out_spawned);
PB_API PbStatus PB_CALL pb_pin_spawn_internal_thread(
    PbThreadRootCallback callback, void* argument, uint64_t stack_size,
    PbThreadId* out_thread_id, PbPinThreadUid* out_thread_uid);
PB_API PbStatus PB_CALL pb_pin_stop_application_threads(
    PbThreadId thread_id, uint8_t* out_stopped);
PB_API PbStatus PB_CALL pb_pin_thread_id(PbThreadId* out_thread_id);
PB_API PbStatus PB_CALL pb_pin_thread_uid(PbPinThreadUid* out_thread_uid);
PB_API PbStatus PB_CALL pb_pin_wait_for_thread_termination(
    PbPinThreadUid thread_uid, uint32_t milliseconds,
    uint8_t* out_terminated, int32_t* out_exit_code);
PB_API PbStatus PB_CALL pb_pin_yield(void);
PB_API PbStatus PB_CALL pb_ptr_diff(
    const void* pointer1, const void* pointer2, uint64_t* out_difference);
PB_API PbStatus PB_CALL pb_uint32_from_string(
    const char* text, uint32_t* out_value);
PB_API PbStatus PB_CALL pb_uint64_from_string(
    const char* text, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pointer_to_addrint(
    void* pointer, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_const_pointer_to_addrint(
    const void* pointer, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_round_down_addr(
    uint64_t address, uint64_t alignment, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_round_down_u64(
    uint64_t value, uint64_t alignment, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_round_up_addr(
    uint64_t address, uint64_t alignment, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_round_up_u64(
    uint64_t value, uint64_t alignment, uint64_t* out_value);

PB_API PbStatus PB_CALL pb_reformat(
    const char* text, const char* prefix, uint32_t min_line, uint32_t max_line,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_bignum(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_bool(
    uint8_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_dec(
    uint64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_dec_signed(
    int64_t value, uint32_t digits, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_flt(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_from_addrint(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_from_uint64(
    uint64_t value, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_hex(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_hex32(
    uint32_t value, uint32_t digits, uint8_t prefix_0x,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_string_tri(
    PbTri value, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_left_justify(
    const char* text, uint32_t width, char padding,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_pointer_string(
    const void* pointer, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_i16(
    int16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_i32(
    int32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_i64(
    int64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_u16(
    uint16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_u32(
    uint32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_decstr_u64(
    uint64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_fltstr(
    double value, uint32_t precision, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_i16(
    int16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_i32(
    int32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_i64(
    int64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_u16(
    uint16_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_u32(
    uint32_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_u64(
    uint64_t value, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_pointer(
    void* pointer, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_hexstr_const_pointer(
    const void* pointer, uint32_t width,
    char* buffer, uint64_t capacity, uint64_t* required_size);

/* PB-PIN-CONTROL-0076 / PIN_Version. required_size includes the NUL. */
PB_API PbStatus PB_CALL pb_pin_version(char* buffer, uint64_t capacity, uint64_t* required_size);

/* PB-PIN-CONTROL-0051 / PIN_Init. argv contains mutable UTF-8 buffers. */
PB_API PbStatus PB_CALL pb_pin_init(int32_t argc, char** argv);

/* PB-PIN-CONTROL-0069 / PIN_StartProgram with Pin's default configuration. */
PB_API PB_NORETURN void PB_CALL pb_pin_start_program_default(void);
PB_API PB_NORETURN void PB_CALL pb_pin_start_program_configured(
    PbPinConfigurationHandle configuration);

PB_API PbStatus PB_CALL pb_pin_init_symbols(void);
PB_API PbStatus PB_CALL pb_pin_init_symbols_alt(
    PbSymbolInfoMode mode, uint8_t* out_success);
PB_API PbStatus PB_CALL pb_pin_lock_client(void);
PB_API PbStatus PB_CALL pb_pin_unlock_client(void);
PB_API PbStatus PB_CALL pb_pin_set_smc_support(PbSmcMode mode);
PB_API PbStatus PB_CALL pb_pin_create_default_configuration_info(
    PbPinConfigurationHandle* out_configuration);
PB_API PB_NORETURN void PB_CALL pb_pin_start_program_probed(void);

/* JIT-only CONTROL operations. Range is the half-open interval [start, end). */
PB_API PbStatus PB_CALL pb_pin_remove_fini_functions(void);
PB_API PbStatus PB_CALL pb_pin_remove_instrumentation(void);
PB_API PbStatus PB_CALL pb_pin_remove_instrumentation_in_range(
    uint64_t start, uint64_t end);

/* PB-PIN-INS_INSTRUMENTATION-0001 / INS_AddInstrumentFunction */
PB_API PbStatus PB_CALL pb_ins_add_instrument_function(
    PbInsInstrumentCallback callback,
    void* user_data,
    PbCallbackHandle* out_callback);

/* JIT-only fixed INS instrumentation. Call/IF/THEN entries expand to
   IPOINT_BEFORE, IARG_PTR(user_data), IARG_END. Predicated entries preserve
   Pin's predicate-aware ordering. Fill entries expand to IARG_INST_PTR,
   field_offset, IARG_END. */
PB_API PbStatus PB_CALL pb_ins_insert_call_before(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
/* IPOINT_BEFORE + IARG_CONTEXT, IARG_PTR(user_data), IARG_END (ABI v1.3).
   The callback runs with the pre-instruction thread context and may
   redirect execution with pb_pin_execute_at. */
PB_API PbStatus PB_CALL pb_ins_insert_call_before_ctx(
    PbInsHandle ins, PbInsContextAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_if_call_before(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_then_call_before(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_predicated_call_before(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_if_predicated_call_before(
    PbInsHandle ins, PbInsPredicateCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_then_predicated_call_before(
    PbInsHandle ins, PbInsAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_fill_buffer(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t field_offset);
PB_API PbStatus PB_CALL pb_ins_insert_fill_buffer_predicated(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t field_offset);
PB_API PbStatus PB_CALL pb_ins_insert_fill_buffer_then(
    PbInsHandle ins, PbIpoint ipoint, PbBufferId id, uint32_t field_offset);

/* JIT-only fixed capture instrumentation (ABI v1.1). These entries let C ABI
   consumers (Rust, ...) build event engines without variadic INS_InsertCall.
   Each expands to IPOINT_BEFORE with a fixed IARG capture list:
   - pb_ins_insert_capture_regs: IARG_INST_PTR, IARG_THREAD_ID and four
     architecture-specific general-purpose register slots (RCX/RDX/R8/R9
     on Win64; ECX/EDX/EAX/EBX on ia32).
   - pb_ins_insert_memory_operands: one predicated call per memory operand
     carrying IARG_MEMORYOP_EA/SIZE and a PB_MEMORY_TYPE_* access tag.
   - pb_ins_insert_exec: IARG_INST_PTR, IARG_THREAD_ID and the static
     instruction size.
   - pb_ins_insert_branch_edge: IARG_INST_PTR, IARG_THREAD_ID,
     IARG_BRANCH_TARGET_ADDR and IARG_BRANCH_TAKEN. */
typedef void (PB_CALL* PbInsCaptureRegsCallback)(
    uint64_t address, uint32_t thread_id,
    uint64_t rcx, uint64_t rdx, uint64_t r8, uint64_t r9, void* user_data);
/* Context-bearing variant for synchronous Hook actions. The register values
   are captured without querying Pin from the application thread; the borrowed
   context may be changed with pb_pin_set_context_reg or the ABI-aware stack
   argument helpers and committed with pb_pin_execute_at. */
typedef void (PB_CALL* PbInsContextCaptureRegsCallback)(
    uint64_t address, uint32_t thread_id, PbContextHandle context,
    uint64_t rcx, uint64_t rdx, uint64_t r8, uint64_t r9, void* user_data);
typedef void (PB_CALL* PbInsMemoryOperandCallback)(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t access, void* user_data);
typedef uint64_t (PB_CALL* PbInsMemoryTranslateCallback)(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t memory_operation,
    void* user_data);
typedef void (PB_CALL* PbInsExecCallback)(
    uint64_t address, uint32_t thread_id, uint32_t size, void* user_data);
typedef void (PB_CALL* PbInsBranchEdgeCallback)(
    uint64_t address, uint32_t thread_id,
    uint64_t target_address, uint64_t taken, void* user_data);

PB_API PbStatus PB_CALL pb_ins_insert_capture_regs(
    PbInsHandle ins, PbInsCaptureRegsCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_capture_regs_ctx(
    PbInsHandle ins, PbInsContextCaptureRegsCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_memory_operands(
    PbInsHandle ins, PbInsMemoryOperandCallback callback, void* user_data);
/* ABI v1.7: for each non-scattered memory operand, call the translator at
   IPOINT_BEFORE, place its returned address in scratch_reg0/1, and rewrite
   the application operand to that register. memory_operation is
   PB_PIN_MEMOP_LOAD or PB_PIN_MEMOP_STORE (RMW is classified as STORE).
   The caller must claim two distinct tool registers before instrumentation. */
PB_API PbStatus PB_CALL pb_ins_insert_memory_address_translation(
    PbInsHandle ins, PbInsMemoryTranslateCallback callback, void* user_data,
    PbRegId scratch_reg0, PbRegId scratch_reg1);
PB_API PbStatus PB_CALL pb_ins_insert_exec(
    PbInsHandle ins, PbInsExecCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_branch_edge(
    PbInsHandle ins, PbInsBranchEdgeCallback callback, void* user_data);

/* Value-capturing extensions of the fixed capture family (ABI v1.5), built
   for the trace recording channel. Same IPOINT_BEFORE predicated-call
   expansion and (address, thread_id, ...) argument order as the v1.1
   entries; the callback never fails -- a short PIN_SafeCopy passes what it
   got, zero-padded.
   - pb_ins_insert_capture_exec_bytes: IARG_INST_PTR, IARG_THREAD_ID, the
     static instruction size, and up to 15 instruction bytes safe-copied at
     analysis time (bytes_lo = bytes[0..8), bytes_hi = bytes[8..15) in the
     low bytes, zero-padded). On copy failure the size goes through with
     zeroed bytes.
   - pb_ins_insert_memory_operands_values: one predicated call per memory
     operand like pb_ins_insert_memory_operands, extended with value = up to
     8 bytes safe-copied from the effective address (zero-padded). Read
     operands report the value about to be read at IPOINT_BEFORE. Write
     operands report the value just written from IPOINT_AFTER when
     INS_IsValidForIpointAfter holds (the pre-write EA is carried over
     thread-locally; IARG_MEMORYOP_EA itself is IPOINT_BEFORE-only); when it
     does not hold (no fall-through, e.g. call) the call stays at
     IPOINT_BEFORE and value is the pre-write content. */
typedef void (PB_CALL* PbInsExecBytesCallback)(
    uint64_t address, uint32_t thread_id, uint32_t size,
    uint64_t bytes_lo, uint64_t bytes_hi, void* user_data);
typedef void (PB_CALL* PbInsMemoryOperandValueCallback)(
    uint64_t instruction_address, uint32_t thread_id,
    uint64_t memory_address, uint32_t size, uint32_t access,
    uint64_t value, void* user_data);

PB_API PbStatus PB_CALL pb_ins_insert_capture_exec_bytes(
    PbInsHandle ins, PbInsExecBytesCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_ins_insert_memory_operands_values(
    PbInsHandle ins, PbInsMemoryOperandValueCallback callback, void* user_data);

/* Runtime disassembly (ABI v1.2): decodes a caller-provided byte image with
   XED. Does not read target memory itself -- the caller supplies the bytes.
   text is the Intel-syntax instruction, NUL-terminated and truncated. */
typedef struct PbDisasmInsn
{
    uint64_t address;
    uint32_t size;
    uint32_t kind; /* 0 linear, 1 branch, 2 call, 3 return */
    char text[64];
} PbDisasmInsn;

PB_API PbStatus PB_CALL pb_disassemble(
    const uint8_t* bytes, uint64_t size, uint64_t address,
    PbDisasmInsn* out, uint64_t capacity, uint64_t* out_count);

/* Control-flow classification of one instruction (ABI v1.4), for exact
   single-stepping: enumerates every possible successor, either directly
   (target) or through a register/memory expression the caller evaluates
   against a live stopped context. */
typedef struct PbFlowInsn
{
    uint64_t address;
    uint32_t size;
    uint32_t kind;       /* 0 linear, 1 branch, 2 call, 3 return */
    uint8_t conditional; /* address+size is also a possible successor */
    uint8_t has_target;  /* target holds a direct successor address */
    uint8_t ind_reg;     /* a successor is the value of base_reg */
    uint8_t ind_mem;     /* a successor is *(base + index*scale + disp) */
    int32_t base_reg;    /* PbRegId or -1; PB_REG_RIP means base = address+size */
    int32_t index_reg;   /* PbRegId or -1 */
    uint64_t scale;
    int64_t disp;
    uint64_t target;     /* direct successor when has_target != 0 */
} PbFlowInsn;

PB_API PbStatus PB_CALL pb_disassemble_flow(
    const uint8_t* bytes, uint64_t size, uint64_t address, PbFlowInsn* out);

/* Remaining INS inspection APIs. Strings are UTF-8 and required_size includes
   the terminating NUL. The XED decode handle is borrowed for the INS callback. */
PB_API PbStatus PB_CALL pb_category_string_short(
    uint32_t category, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_extension_string_short(
    uint32_t extension, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_opcode_string_short(
    uint32_t opcode, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_ins_disassemble(
    PbInsHandle ins, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_ins_mnemonic(
    PbInsHandle ins, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_ins_get_number_and_size_of_mem_accesses(
    PbInsHandle ins, int32_t* out_num_accesses, int32_t* out_access_size,
    int32_t* out_index_size);
PB_API PbStatus PB_CALL pb_ins_change_reg(
    PbInsHandle ins, PbRegId old_reg, PbRegId new_reg, uint8_t as_read,
    uint8_t* out_changed);
PB_API PbStatus PB_CALL pb_ins_get_far_pointer(
    PbInsHandle ins, uint16_t* out_segment_selector, uint32_t* out_displacement);
PB_API PbStatus PB_CALL pb_ins_invalid(PbInsHandle* out_ins);
PB_API PbStatus PB_CALL pb_ins_xed_dec(
    PbInsHandle ins, PbXedDecodedInstHandle* out_decoded_instruction);
PB_API PbStatus PB_CALL pb_ins_xed_exact_map_from_pin_reg(
    PbRegId pin_reg, PbXedRegId* out_xed_reg);
PB_API PbStatus PB_CALL pb_ins_xed_exact_map_to_pin_reg(
    PbXedRegId xed_reg, PbRegId* out_pin_reg);
PB_API PbStatus PB_CALL pb_ins_xed_exact_map_to_pin_reg_legacy(
    uint32_t xed_reg, PbRegId* out_pin_reg);
PB_API PbStatus PB_CALL pb_pin_set_syntax_att(void);
PB_API PbStatus PB_CALL pb_pin_set_syntax_intel(void);
PB_API PbStatus PB_CALL pb_pin_set_syntax_xed(void);

/* Fixed-signature Pin 3.31 INS inspection queries. */
#define PB_INS_C_TYPE_BOOL uint8_t
#define PB_INS_C_TYPE_INT32 int32_t
#define PB_INS_C_TYPE_UINT32 uint32_t
#define PB_INS_C_TYPE_UINT64 uint64_t
#define PB_INS_C_TYPE_USIZE uint64_t
#define PB_INS_C_TYPE_ADDRINT uint64_t
#define PB_INS_C_TYPE_ADDRDELTA int64_t
#define PB_INS_C_TYPE_REG PbRegId
#define PB_INS_C_TYPE_INS PbInsHandle
#define PB_INS_C_TYPE_RTN PbRtnHandle
#define PB_INS_C_TYPE_PREDICATE PbPredicate
#define PB_INS_C_TYPE_SYSCALL_STANDARD uint32_t
#define PB_INS_C_TYPE_OPCODE uint32_t
#define PB_INS_C_ARG_UINT32 uint32_t
#define PB_INS_C_ARG_REG PbRegId
#define PB_INS_C_ARG_IARG_TYPE PbIargType
#define PB_INS_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol(PbInsHandle ins, PB_INS_C_TYPE_##return_kind* out_value);
#define PB_INS_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol( \
        PbInsHandle ins, PB_INS_C_ARG_##argument_kind argument, \
        PB_INS_C_TYPE_##return_kind* out_value);
#include "pinbridge/generated/ins_inspection_queries.inc"
#undef PB_INS_QUERY1
#undef PB_INS_QUERY0

/* Pure fixed-signature Pin 3.31 REG queries. */
#define PB_REG_C_TYPE_BOOL uint8_t
#define PB_REG_C_TYPE_REG PbRegId
#define PB_REG_C_TYPE_UINT32 uint32_t
#define PB_REG_C_TYPE_REGWIDTH uint32_t
#define PB_REG_C_TYPE_UINT8 uint8_t
#define PB_REG_C_TYPE_ADDRINT uint64_t
#define PB_REG_C_ARG_REG PbRegId
#define PB_REG_C_ARG_UINT16 uint16_t
#define PB_REG_QUERY0(return_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol(PB_REG_C_TYPE_##return_kind* out_value);
#define PB_REG_QUERY1(return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol( \
        PB_REG_C_ARG_##argument_kind argument, PB_REG_C_TYPE_##return_kind* out_value);
#include "pinbridge/generated/reg_queries.inc"
#undef PB_REG_QUERY1
#undef PB_REG_QUERY0

/* Pin 3.31 REGSET fixed operations. Callers own and synchronize each set. */
PB_API PbStatus PB_CALL pb_regset_add_all(PbRegSet* set);
PB_API PbStatus PB_CALL pb_regset_clear(PbRegSet* set);
PB_API PbStatus PB_CALL pb_regset_contains(
    const PbRegSet* set, PbRegId reg, uint8_t* out_contains);
PB_API PbStatus PB_CALL pb_regset_insert(PbRegSet* set, PbRegId reg);
PB_API PbStatus PB_CALL pb_regset_pop_count(const PbRegSet* set, uint32_t* out_count);
PB_API PbStatus PB_CALL pb_regset_is_empty(const PbRegSet* set, uint8_t* out_is_empty);
PB_API PbStatus PB_CALL pb_regset_pop_next(PbRegSet* set, PbRegId* out_reg);
PB_API PbStatus PB_CALL pb_regset_remove(PbRegSet* set, PbRegId reg);
PB_API PbStatus PB_CALL pb_regset_first_reg(PbRegId* out_reg);
PB_API PbStatus PB_CALL pb_regset_last_reg(PbRegId* out_reg);
/* ASCII register names. required_size includes the terminating NUL. */
PB_API PbStatus PB_CALL pb_regset_string_short(
    const PbRegSet* set, char* buffer, uint64_t capacity, uint64_t* required_size);

/* Remaining fixed REG functions. Output buffers are caller-owned. */
PB_API PbStatus PB_CALL pb_pin_claim_tool_register(PbRegId* out_reg);
PB_API PbStatus PB_CALL pb_reg_convert_x87_abridged_tag_to_full(
    const PbFxSave* fxsave, uint16_t* out_full_tag);
PB_API PbStatus PB_CALL pb_reg_string_short(
    PbRegId reg, char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_reg_prefix_increment(PbRegId* reg, PbRegId* out_result);
PB_API PbStatus PB_CALL pb_reg_postfix_increment(PbRegId* reg, PbRegId* out_previous);
PB_API PbStatus PB_CALL pb_reg_postfix_decrement(PbRegId* reg, PbRegId* out_previous);

/* Fixed-signature BBL/TRACE/RTN/SEC/IMG inspection queries. */
#define PB_HANDLE_C_INPUT_BBL PbBblHandle
#define PB_HANDLE_C_INPUT_TRACE PbTraceHandle
#define PB_HANDLE_C_INPUT_RTN PbRtnHandle
#define PB_HANDLE_C_INPUT_SEC PbSecHandle
#define PB_HANDLE_C_INPUT_IMG PbImgHandle
#define PB_HANDLE_C_TYPE_ADDRINT uint64_t
#define PB_HANDLE_C_TYPE_BOOL uint8_t
#define PB_HANDLE_C_TYPE_INS PbInsHandle
#define PB_HANDLE_C_TYPE_BBL PbBblHandle
#define PB_HANDLE_C_TYPE_UINT32 uint32_t
#define PB_HANDLE_C_TYPE_UINT uint32_t
#define PB_HANDLE_C_TYPE_USIZE uint64_t
#define PB_HANDLE_C_TYPE_RTN PbRtnHandle
#define PB_HANDLE_C_TYPE_SEC PbSecHandle
#define PB_HANDLE_C_TYPE_IMG PbImgHandle
#define PB_HANDLE_C_TYPE_SYM PbSymHandle
#define PB_HANDLE_C_TYPE_SEC_TYPE PbSecType
#define PB_HANDLE_C_TYPE_IMG_TYPE PbImgType
#define PB_HANDLE_C_ARG_UINT32 uint32_t
#define PB_HANDLE_C_ARG_PROBE_MODE uint32_t
#define PB_HANDLE_C_ARG_IMG_PROPERTY PbImgProperty
#define PB_HANDLE_QUERY0(input_kind, return_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol( \
        PB_HANDLE_C_INPUT_##input_kind input, PB_HANDLE_C_TYPE_##return_kind* out_value);
#define PB_HANDLE_QUERY1(input_kind, return_kind, argument_kind, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol( \
        PB_HANDLE_C_INPUT_##input_kind input, PB_HANDLE_C_ARG_##argument_kind argument, \
        PB_HANDLE_C_TYPE_##return_kind* out_value);
#include "pinbridge/generated/structure_queries.inc"
#undef PB_HANDLE_QUERY1
#undef PB_HANDLE_QUERY0

/* SEC_Data returns borrowed pointer bits. In an image callback the address is
   invalid after callback return; for IMG_Open it remains valid until IMG_Close. */
PB_API PbStatus PB_CALL pb_sec_data(PbSecHandle sec, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_sec_invalid(PbSecHandle* out_sec);
/* UTF-8. required_size includes NUL; no partial write on small buffers. */
PB_API PbStatus PB_CALL pb_sec_name(
    PbSecHandle sec, char* buffer, uint64_t capacity, uint64_t* required_size);

/* RTN and IMG tokens are borrowed. Names are UTF-8; required_size includes
   the NUL and small buffers are not partially filled. */
/* At most one routine may be open through the bridge. The same handle must
   be closed before another routine is opened or created. RTN_CreateAt keeps
   the returned borrowed token valid while its containing image is loaded. */
PB_API PbStatus PB_CALL pb_rtn_close(PbRtnHandle routine);
PB_API PbStatus PB_CALL pb_rtn_create_at(
    uint64_t address, const char* name, PbRtnHandle* out_routine);
PB_API PbStatus PB_CALL pb_rtn_find_by_address(
    uint64_t address, PbRtnHandle* out_routine);
PB_API PbStatus PB_CALL pb_rtn_find_by_name(
    PbImgHandle image, const char* name, PbRtnHandle* out_routine);
PB_API PbStatus PB_CALL pb_rtn_find_name_by_address(
    uint64_t address, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_rtn_funptr(
    PbRtnHandle routine, uint64_t* out_function_address);
PB_API PbStatus PB_CALL pb_rtn_invalid(PbRtnHandle* out_routine);
PB_API PbStatus PB_CALL pb_rtn_name(
    PbRtnHandle routine, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_rtn_open(PbRtnHandle routine);
/* Replacement addresses refer to functions in the PinTool. Original addresses
   are application entry points returned by Pin and follow Pin's mode-specific
   calling restrictions. Probe mode flags may be combined. */
PB_API PbStatus PB_CALL pb_rtn_replace(
    PbRtnHandle routine, uint64_t replacement_address,
    uint64_t* out_original_address);
PB_API PbStatus PB_CALL pb_rtn_replace_probed(
    PbRtnHandle routine, uint64_t replacement_address,
    uint64_t* out_original_address);
PB_API PbStatus PB_CALL pb_rtn_replace_probed_ex(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    uint64_t* out_original_address);
PB_API PbStatus PB_CALL pb_rtn_insert_call(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments);
PB_API PbStatus PB_CALL pb_rtn_insert_call_probed(
    PbRtnHandle routine, PbIpoint point, uint64_t callback_address,
    PbIargListHandle arguments, uint8_t* out_inserted);
PB_API PbStatus PB_CALL pb_rtn_insert_call_probed_ex(
    PbRtnHandle routine, PbIpoint point, PbProbeMode mode,
    uint64_t callback_address, PbIargListHandle arguments,
    uint8_t* out_inserted);
PB_API PbStatus PB_CALL pb_rtn_replace_signature(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);
PB_API PbStatus PB_CALL pb_rtn_replace_signature_probed(
    PbRtnHandle routine, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);
PB_API PbStatus PB_CALL pb_rtn_replace_signature_probed_ex(
    PbRtnHandle routine, PbProbeMode mode, uint64_t replacement_address,
    PbIargListHandle arguments, uint64_t* out_original_address);

/* Callback registrations remain active for the Pin process lifetime. */
PB_API PbStatus PB_CALL pb_trace_add_instrument_function(
    PbTraceInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_rtn_add_instrument_function(
    PbRtnInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_img_add_instrument_function(
    PbImgInstrumentCallback callback, void* user_data, PbCallbackHandle* out_callback);

/* IMG list tokens are borrowed. IMG_Open returns a bridge-tracked token that
   must be released by pb_img_close before another image can be opened. Names
   are UTF-8 and required_size includes the NUL. */
PB_API PbStatus PB_CALL pb_app_img_head(PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_app_img_tail(PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_img_add_unload_function(
    PbImgInstrumentCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_img_close(PbImgHandle image);
PB_API PbStatus PB_CALL pb_img_find_by_address(
    uint64_t address, PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_img_find_by_id(
    uint32_t id, PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_img_invalid(PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_img_name(
    PbImgHandle image, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_img_open(
    const char* filename, PbImgHandle* out_image);

/* SYM tokens are borrowed from an image symbol list. Names use UTF-8;
   required_size includes the NUL and small buffers are not partially filled. */
PB_API PbStatus PB_CALL pb_pin_undecorate_symbol_name(
    const char* symbol_name, PbUndecoration style,
    char* buffer, uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_sym_address(
    PbSymHandle symbol, uint64_t* out_address);
PB_API PbStatus PB_CALL pb_sym_dynamic(
    PbSymHandle symbol, uint8_t* out_dynamic);
PB_API PbStatus PB_CALL pb_sym_generated_by_pin(
    PbSymHandle symbol, uint8_t* out_generated);
PB_API PbStatus PB_CALL pb_sym_index(
    PbSymHandle symbol, uint32_t* out_index);
PB_API PbStatus PB_CALL pb_sym_invalid(PbSymHandle* out_symbol);
PB_API PbStatus PB_CALL pb_sym_name(
    PbSymHandle symbol, char* buffer, uint64_t capacity,
    uint64_t* required_size);
PB_API PbStatus PB_CALL pb_sym_next(
    PbSymHandle symbol, PbSymHandle* out_symbol);
PB_API PbStatus PB_CALL pb_sym_prev(
    PbSymHandle symbol, PbSymHandle* out_symbol);
PB_API PbStatus PB_CALL pb_sym_valid(
    PbSymHandle symbol, uint8_t* out_valid);
PB_API PbStatus PB_CALL pb_sym_value(
    PbSymHandle symbol, uint64_t* out_value);

/* Bounded replacement-prototype builder. descriptor_count includes one
   trailing END descriptor and is limited to PB_PROTO_MAX_ARGUMENTS + 1. */
PB_API PbStatus PB_CALL pb_proto_arg_for_kind(
    PbProtoArgKind kind, PbProtoArg* out_arg);
PB_API PbStatus PB_CALL pb_proto_arg_aggregate(
    uint64_t size, PbProtoArg* out_arg);
PB_API PbStatus PB_CALL pb_proto_arg_end(PbProtoArg* out_arg);
PB_API PbStatus PB_CALL pb_proto_arg_enum(
    uint64_t size, PbProtoArg* out_arg);
PB_API PbStatus PB_CALL pb_proto_allocate(
    PbProtoArg return_arg, PbCallingStandard calling_standard,
    const char* name, const PbProtoArg* descriptors,
    uint32_t descriptor_count, PbProtoHandle* out_proto);
PB_API PbStatus PB_CALL pb_proto_free(PbProtoHandle proto);
PB_API PbStatus PB_CALL pb_iarg_list_alloc(PbIargListHandle* out_list);
PB_API PbStatus PB_CALL pb_iarg_list_add(
    PbIargListHandle list, const PbIargDescriptor* descriptors,
    uint32_t descriptor_count);
PB_API PbStatus PB_CALL pb_iarg_list_free(PbIargListHandle list);

/* JIT-only syscall callbacks and context operations. Context pointers are
   borrowed for the callback or operation window and must not be retained. */
PB_API PbStatus PB_CALL pb_pin_add_syscall_entry_function(
    PbSyscallEntryCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_syscall_exit_function(
    PbSyscallExitCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_get_syscall_argument(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_get_syscall_errno(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_get_syscall_number(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_get_syscall_return(
    PbConstContextHandle context, PbSyscallStandard standard,
    uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_replay_syscall_entry(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard);
PB_API PbStatus PB_CALL pb_pin_replay_syscall_exit(
    PbThreadId thread_id, PbContextHandle context,
    PbSyscallStandard standard);
PB_API PbStatus PB_CALL pb_pin_set_syscall_argument(
    PbContextHandle context, PbSyscallStandard standard,
    uint32_t arg_num, uint64_t value);
PB_API PbStatus PB_CALL pb_pin_set_syscall_errno(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);
PB_API PbStatus PB_CALL pb_pin_set_syscall_number(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);
PB_API PbStatus PB_CALL pb_pin_set_syscall_return(
    PbContextHandle context, PbSyscallStandard standard, uint64_t value);

/* JIT-only fixed BBL instrumentation family. Each call expands to
   IPOINT_BEFORE, IARG_PTR(user_data), IARG_END. An if call must be followed
   immediately by its then call within the same instrumentation callback. */
PB_API PbStatus PB_CALL pb_bbl_insert_call_before(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_bbl_insert_if_call_before(
    PbBblHandle bbl, PbBblPredicateCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_bbl_insert_then_call_before(
    PbBblHandle bbl, PbBblAnalysisCallback callback, void* user_data);

/* JIT-only fixed TRACE instrumentation family. Each call expands to
   IPOINT_BEFORE, IARG_PTR(user_data), IARG_END. An if call must be followed
   immediately by its then call within the same instrumentation callback. */
PB_API PbStatus PB_CALL pb_trace_insert_call_before(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_trace_insert_if_call_before(
    PbTraceHandle trace, PbTracePredicateCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_trace_insert_then_call_before(
    PbTraceHandle trace, PbTraceAnalysisCallback callback, void* user_data);

/* JIT-only process-lifetime registration. Enabling Pin SMC tracking may retain
   unbounded tracking state, as documented by the Pin SDK. */
PB_API PbStatus PB_CALL pb_trace_add_smc_detected_function(
    PbTraceSmcCallback callback, void* user_data);

/* JIT-only trace versioning. The fixed version-case family expands either to
   IARG_END or to IARG_CALL_ORDER, call_order, IARG_END. */
PB_API PbStatus PB_CALL pb_bbl_set_target_version(
    PbBblHandle bbl, uint64_t version);
PB_API PbStatus PB_CALL pb_trace_version(
    PbTraceHandle trace, uint64_t* out_version);
PB_API PbStatus PB_CALL pb_ins_insert_version_case(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version);
PB_API PbStatus PB_CALL pb_ins_insert_version_case_with_call_order(
    PbInsHandle ins, PbRegId reg, int32_t case_value, uint64_t version,
    PbCallOrder call_order);

/* JIT-only INS modification.  Jump insertion supports only BEFORE/AFTER.
   Memory rewriting validates the operand against the decoded instruction;
   scattered rewriting additionally requires a gather/scatter operand and a
   matching IARG_REWRITE_SCATTERED_MEMOP analysis callback in the tool. */
PB_API PbStatus PB_CALL pb_ins_delete(PbInsHandle ins);
PB_API PbStatus PB_CALL pb_ins_insert_direct_jump(
    PbInsHandle ins, PbIpoint ipoint, uint64_t target);
PB_API PbStatus PB_CALL pb_ins_insert_indirect_jump(
    PbInsHandle ins, PbIpoint ipoint, PbRegId reg);
PB_API PbStatus PB_CALL pb_ins_rewrite_memory_operand(
    PbInsHandle ins, uint32_t memindex, PbRegId reg);
PB_API PbStatus PB_CALL pb_ins_rewrite_scattered_memory_operand(
    PbInsHandle ins, uint32_t memindex);

/* JIT-only trace-buffer ownership API.  Define retains callback/user_data for
   the Pin process lifetime.  Explicit allocation is needed only for tools
   implementing double buffering. */
PB_API PbStatus PB_CALL pb_pin_define_trace_buffer(
    uint64_t record_size, uint32_t num_pages,
    PbTraceBufferCallback callback, void* user_data, PbBufferId* out_id);
PB_API PbStatus PB_CALL pb_pin_allocate_buffer(
    PbBufferId id, void** out_buffer);
PB_API PbStatus PB_CALL pb_pin_deallocate_buffer(
    PbBufferId id, void* buffer);
PB_API PbStatus PB_CALL pb_pin_get_buffer_pointer(
    PbContextHandle context, PbBufferId id, void** out_buffer);

/* JIT/Probe error-file reporting. Strings are borrowed UTF-8 NUL-terminated
   values. Fatal severity does not return after Pin accepts the message. */
PB_API PbStatus PB_CALL pb_pin_write_error_message(
    const char* message, int32_t type, PbPinErrorSeverity severity,
    const char* const* arguments, uint32_t argument_count);

/* JIT-only callback property access. Lower execution orders run earlier among
   similar callbacks. Arbitrary PbCallOrder values are preserved. */
PB_API PbStatus PB_CALL pb_callback_get_execution_order(
    PbCallbackHandle callback, PbCallOrder* out_order);
PB_API PbStatus PB_CALL pb_callback_set_execution_order(
    PbCallbackHandle callback, PbCallOrder order);

/* Deprecated Pin spellings retained as distinct ABI entries. The priority
   pair is JIT-only; IMG_Entry is available in JIT and Probe mode. */
PB_API PbStatus PB_CALL pb_callback_get_execution_priority_deprecated(
    PbCallbackHandle callback, int32_t* out_priority);
PB_API PbStatus PB_CALL pb_callback_set_execution_priority_deprecated(
    PbCallbackHandle callback, int32_t priority);
PB_API PbStatus PB_CALL pb_img_entry_deprecated(
    PbImgHandle image, uint64_t* out_entry);

/* JIT-only replay support. A successful context replay does not return; the
   status return exists for validation and backend failures before transfer. */
PB_API PbStatus PB_CALL pb_img_create_at(
    const char* filename, uint64_t start, uint64_t size, uint64_t load_offset,
    uint8_t main_executable, PbImgHandle* out_image);
PB_API PbStatus PB_CALL pb_img_replay_image_load(PbImgHandle image);
PB_API PbStatus PB_CALL pb_pin_replay_context_change(
    PbThreadId thread_id, PbConstContextHandle from, PbContextHandle to,
    PbContextChangeReason reason, int32_t info);

/* CONTROL lifecycle registrations remain active for the Pin process lifetime. */
PB_API PbStatus PB_CALL pb_pin_add_application_start_function(
    PbApplicationStartCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_prepare_for_fini_function(
    PbPrepareForFiniCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_fini_function(
    PbFiniCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_thread_start_function(
    PbThreadStartCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_thread_fini_function(
    PbThreadFiniCallback callback, void* user_data, PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_context_change_function(
    PbContextChangeCallback callback, void* user_data, PbCallbackHandle* out_callback);
/* Pin owns one global XED decode callback slot; repeated calls replace bridge state. */
PB_API PbStatus PB_CALL pb_pin_add_xed_decode_callback_function(
    PbXedDecodeCallback callback, void* user_data);
/* ABI v1.9: safely changes supported inputs on the borrowed XED object.
   selected_features says which inputs to change; enabled_features supplies
   their Boolean values and must be a subset of selected_features. */
PB_API PbStatus PB_CALL pb_xed_decoded_inst_set_features(
    PbXedDecodedInstHandle decoded_instruction,
    uint32_t selected_features, uint32_t enabled_features);
PB_API PbStatus PB_CALL pb_pin_add_fetch_function(
    PbFetchCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_pin_fetch_code(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info, uint64_t* out_copied);
/* ABI v1.8: raw fallback for use inside PbFetchCallback. Unlike
   pb_pin_fetch_code, this never invokes the registered fetch callback. */
PB_API PbStatus PB_CALL pb_pin_fetch_original_code(
    void* copy_buffer, uint64_t address, uint64_t max_size,
    PbExceptionInfoHandle exception_info, uint64_t* out_copied);
PB_API PbStatus PB_CALL pb_pin_add_internal_exception_handler(
    PbInternalExceptionCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_try_start(
    PbThreadId thread_id, PbInternalExceptionCallback callback, void* user_data,
    PbCallbackHandle* out_scope);
PB_API PbStatus PB_CALL pb_pin_try_end(
    PbThreadId thread_id, PbCallbackHandle* scope);
PB_API PbStatus PB_CALL pb_pin_add_memory_address_trans_function(
    PbMemoryAddressTransCallback callback, void* user_data);
PB_API PbStatus PB_CALL pb_pin_get_memory_address_trans_function(
    PbMemoryAddressTransCallback* out_callback);
/* Process-global single slot. NULL callback disables notification. The callback
   may run concurrently, with unknown lock state, and must not allocate memory. */
PB_API PbStatus PB_CALL pb_pin_add_out_of_memory_function(
    PbOutOfMemoryCallback callback, void* user_data);
/* Only one follow-child callback may be registered in a Pin process. */
PB_API PbStatus PB_CALL pb_pin_add_follow_child_process_function(
    PbFollowChildProcessCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
/* Child command-line data is borrowed by Pin and exposed through copies only.
   Bytes use Pin's narrow-character encoding without transcoding. required_size
   includes NUL and small buffers are untouched. */
PB_API PbStatus PB_CALL pb_child_process_get_command_line_count(
    PbChildProcessHandle child, int32_t* out_argc);
PB_API PbStatus PB_CALL pb_child_process_get_command_line_argument(
    PbChildProcessHandle child, int32_t index, char* buffer,
    uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_child_process_get_id(
    PbChildProcessHandle child, uint32_t* out_process_id);
/* argv is consumed synchronously and remains owned by the caller. */
PB_API PbStatus PB_CALL pb_child_process_set_pin_command_line(
    PbChildProcessHandle child, int32_t argc, const char* const* argv);
/* Fixed PIN_InsertCallProbed family: IARG_PTR(user_data), IARG_END. */
PB_API PbStatus PB_CALL pb_pin_insert_call_probed(
    uint64_t address, PbProbedCallCallback callback,
    void* user_data, uint8_t* out_inserted);
/* JIT-only fixed PIN_CallApplicationFunction families. The context is borrowed
   from the active Pin callback. Calls use CALLINGSTD_DEFAULT and default
   CALL_APPLICATION_FUNCTION_PARAM settings. */
PB_API PbStatus PB_CALL pb_pin_call_application_function_void_0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address);
PB_API PbStatus PB_CALL pb_pin_call_application_function_u64_0(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t* out_result);
PB_API PbStatus PB_CALL pb_pin_call_application_function_u64_1(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t* out_result);
PB_API PbStatus PB_CALL pb_pin_call_application_function_u64_2(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t argument0, uint64_t argument1,
    uint64_t* out_result);
/* Calls a function with the shape void* target(size_t). */
PB_API PbStatus PB_CALL pb_pin_call_application_function_ptr_usize(
    PbConstContextHandle context, PbThreadId thread_id,
    uint64_t function_address, uint64_t size, void** out_result);
/* JIT and Probe detach completion callbacks are deliberately separate:
   registering the wrong Pin callback family for the active mode is invalid. */
PB_API PbStatus PB_CALL pb_pin_add_detach_function(
    PbDetachCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_add_detach_function_probed(
    PbDetachProbedCallback callback, void* user_data,
    PbCallbackHandle* out_callback);
PB_API PbStatus PB_CALL pb_pin_detach(void);
PB_API PbStatus PB_CALL pb_pin_detach_probed(void);
/* Asynchronous reattach requests. PB_OK reports bridge execution; out_status
   reports whether Pin accepted the request or detach is still completing. */
PB_API PbStatus PB_CALL pb_pin_attach_probed(
    PbAttachProbedCallback callback, void* user_data, PbAttachStatus* out_status);

/* PROCESS queries and exits. ExitApplication is JIT-only and runs Pin Fini
   callbacks; ExitProcess is immediate and supports JIT and Probe. */
PB_API PB_NORETURN void PB_CALL pb_pin_exit_application(int32_t status);
PB_API PB_NORETURN void PB_CALL pb_pin_exit_process(int32_t exit_code);
PB_API PbStatus PB_CALL pb_pin_get_pid(int32_t* out_pid);
PB_API PbStatus PB_CALL pb_pin_is_amx_active(
    PbThreadId thread_id, uint8_t* out_active);
/* tile_config_size must be at least the 64-byte Intel64 TILECONFIG width. */
PB_API PbStatus PB_CALL pb_tile_config_get_palette_id(
    const uint8_t* tile_config, uint64_t tile_config_size, uint8_t* out_palette_id);
PB_API PbStatus PB_CALL pb_tile_config_get_tile_bytes_per_row(
    const uint8_t* tile_config, uint64_t tile_config_size,
    PbRegId tmm, uint32_t* out_bytes_per_row);
PB_API PbStatus PB_CALL pb_tile_config_get_tile_rows(
    const uint8_t* tile_config, uint64_t tile_config_size,
    PbRegId tmm, uint32_t* out_rows);

/* PB-PIN-CONTEXT-0020 / PIN_GetContextReg */
PB_API PbStatus PB_CALL pb_pin_get_context_reg(
    PbConstContextHandle context,
    PbRegId reg,
    uint64_t* out_value);

PB_API PbStatus PB_CALL pb_pin_get_context_regval(
    PbConstContextHandle context, PbRegId reg, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size);
/* PB-PIN-CONTEXT-0022 / PIN_GetFullContextRegsSet */
PB_API PbStatus PB_CALL pb_pin_get_full_context_regs_set(PbRegSet* out_regs);
PB_API PbStatus PB_CALL pb_pin_get_context_fpstate(
    PbConstContextHandle context, uint8_t* buffer,
    uint64_t capacity, uint64_t* required_size);
PB_API PbStatus PB_CALL pb_pin_set_context_fpstate(
    PbContextHandle context, const uint8_t* value, uint64_t value_size);
PB_API PbStatus PB_CALL pb_pin_get_context_fxsave(
    PbConstContextHandle context, PbFxSave* out_fxsave);
PB_API PbStatus PB_CALL pb_pin_set_context_fxsave(
    PbContextHandle context, const PbFxSave* fxsave);

/* Windows JIT-only PHYSICAL_CONTEXT access. Handles are borrowed from an
   internal-exception callback and registers must be physical integers. */
PB_API PbStatus PB_CALL pb_pin_get_physical_context_reg(
    PbConstPhysicalContextHandle context, PbRegId reg, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_set_physical_context_reg(
    PbPhysicalContextHandle context, PbRegId reg, uint64_t value);
PB_API PbStatus PB_CALL pb_pin_get_physical_context_fxsave(
    PbConstPhysicalContextHandle context, PbFxSave* out_fxsave);
PB_API PbStatus PB_CALL pb_pin_set_physical_context_fxsave(
    PbPhysicalContextHandle context, const PbFxSave* fxsave);
/* PB-PIN-CONTEXT-0029 / PIN_SupportsProcessorState */
PB_API PbStatus PB_CALL pb_pin_supports_processor_state(
    PbProcessorState state, uint8_t* out_supported);
/* PB-PIN-CONTEXT-0016 / PIN_ContextContainsState. context is borrowed. */
PB_API PbStatus PB_CALL pb_pin_context_contains_state(
    PbContextHandle context, PbProcessorState state, uint8_t* out_contains);

/* SDK-backed Pin 3.31 CONTEXT constants, widened to uint64_t on Windows x64. */
#define PB_CONTEXT_CONSTANT(index, c_symbol, pin_symbol, api_id) \
    PB_API PbStatus PB_CALL c_symbol(uint64_t* out_value);
#include "pinbridge/generated/context_constants.inc"
#undef PB_CONTEXT_CONSTANT

PB_API PbStatus PB_CALL pb_pin_save_context(
    PbConstContextHandle source, PbContextHandle destination);
PB_API PbStatus PB_CALL pb_pin_set_context_reg(
    PbContextHandle context, PbRegId reg, uint64_t value);
PB_API PbStatus PB_CALL pb_pin_set_context_regval(
    PbContextHandle context, PbRegId reg, const uint8_t* value, uint64_t value_size);
/* ABI-aware integer stack arguments at a function entry. `index` is the
 * logical stack argument number: x86 index 0 is [ESP+4], while x64 index 0
 * is the first argument beyond RCX/RDX/R8/R9 at [RSP+0x28]. */
PB_API PbStatus PB_CALL pb_pin_get_context_stack_arg(
    PbConstContextHandle context, uint32_t index, uint64_t* out_value);
PB_API PbStatus PB_CALL pb_pin_set_context_stack_arg(
    PbContextHandle context, uint32_t index, uint64_t value);

/* PB-PIN-CONTEXT-0017 / PIN_ExecuteAt.
 * NULL returns PB_ERR_INVALID_ARGUMENT. A valid call is only permitted from a
 * Pin analysis or replacement routine and does not return on success. */
PB_API PbStatus PB_CALL pb_pin_execute_at(PbConstContextHandle context);

/* Fixed CONTROL queries. Boolean outputs are normalized to 0 or 1. */
PB_API PbStatus PB_CALL pb_pin_check_read_access(uint64_t address, uint8_t* out_accessible);
PB_API PbStatus PB_CALL pb_pin_check_write_access(uint64_t address, uint8_t* out_accessible);
PB_API PbStatus PB_CALL pb_pin_is_attaching(uint8_t* out_attaching);
PB_API PbStatus PB_CALL pb_pin_is_probe_mode(uint8_t* out_probe_mode);
/* Returns PB_ERR_INVALID_STATE unless Pin is already running in Probe mode. */
PB_API PbStatus PB_CALL pb_pin_is_safe_for_probed_insertion(
    uint64_t address, uint8_t* out_safe);
/* UTF-8. required_size includes NUL; no partial write on small buffers. */
PB_API PbStatus PB_CALL pb_pin_tool_full_path(
    char* buffer, uint64_t capacity, uint64_t* required_size);

/* PB-PIN-DEBUG_INFO-0001 / PIN_GetSourceLocation. file_name is UTF-8. */
PB_API PbStatus PB_CALL pb_pin_get_source_location(
    uint64_t address, int32_t* column, int32_t* line,
    char* file_name, uint64_t capacity, uint64_t* required_size);

/* PB-PIN-CONTROL-0064 / PIN_SafeCopy */
PB_API PbStatus PB_CALL pb_pin_safe_copy(
    void* destination,
    uint64_t source_address,
    uint64_t size,
    uint64_t* out_copied);
PB_API PbStatus PB_CALL pb_pin_safe_copy_ex(
    void* destination,
    uint64_t source_address,
    uint64_t size,
    uint64_t* out_copied,
    PbExceptionInfoSnapshot* out_exception);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* PINBRIDGE_PINBRIDGE_H */
