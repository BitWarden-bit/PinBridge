#include <stddef.h>
#include <stdint.h>

#include "pinbridge/pinbridge.h"

_Static_assert(sizeof(PbStatus) == 4, "PbStatus must be 32-bit");
_Static_assert(sizeof(PbTri) == 4, "PbTri must be 32-bit");
_Static_assert(sizeof(PbInsHandle) == 4, "PbInsHandle must be 32-bit");
_Static_assert(sizeof(PbRtnHandle) == 4, "PbRtnHandle must be 32-bit");
_Static_assert(sizeof(PbBblHandle) == 4, "PbBblHandle must be 32-bit");
_Static_assert(sizeof(PbSecHandle) == 4, "PbSecHandle must be 32-bit");
_Static_assert(sizeof(PbImgHandle) == 4, "PbImgHandle must be 32-bit");
_Static_assert(sizeof(PbSymHandle) == 4, "PbSymHandle must be 32-bit");
_Static_assert(sizeof(PbTraceHandle) == sizeof(void*), "PbTraceHandle must be pointer-sized");
_Static_assert(sizeof(PbCallbackHandle) == 8, "PbCallbackHandle must be 64-bit");
_Static_assert(sizeof(PbContextHandle) == sizeof(void*), "PbContextHandle must be pointer-sized");
_Static_assert(sizeof(PbRegId) == 4, "PbRegId must be 32-bit");
_Static_assert(sizeof(PbRegWidth) == 4, "PbRegWidth must be 32-bit");
_Static_assert(sizeof(PbRegClass) == 4, "PbRegClass must be 32-bit");
_Static_assert(sizeof(PbRegClassBits) == 8, "PbRegClassBits must be 64-bit");
_Static_assert(PB_REGWIDTH_NATIVE == PB_REGWIDTH_64, "native width must be x64");
_Static_assert(PB_REG_ALLOC_CR == PB_REG_ALLOC_IDENT, "allocation alias changed");
_Static_assert(PB_REGCBIT_ALL_REGS ==
                   (PB_REGCBIT_APP_ALL | PB_REGCBIT_PIN_ALL),
               "all-register mask changed");
_Static_assert(PB_REG_METADATA_CONSTANT_COUNT == 100u,
               "REG metadata constant coverage is incomplete");
_Static_assert(PB_PIN_PRODUCT_VERSION_MAJOR == UINT32_C(3),
               "Pin major version changed");
_Static_assert(PB_PIN_PRODUCT_VERSION_MINOR == UINT32_C(31),
               "Pin minor version changed");
_Static_assert(PB_PIN_BUILD_NUMBER == UINT32_C(98869),
               "Pin build number changed");
_Static_assert(PB_REG_INVALID_ == 0u, "REG_INVALID_ value changed");
_Static_assert(PB_REG_NONE == 1u, "REG_NONE value changed");
_Static_assert(PB_REG_GAX == PB_REG_RAX, "REG_GAX alias changed");
_Static_assert(PB_REG_GFLAGS == PB_REG_RFLAGS, "REG_GFLAGS alias changed");
_Static_assert(PB_REG_INST_PTR == PB_REG_RIP, "REG_INST_PTR alias changed");
_Static_assert(PB_REG_RAX != PB_REG_EAX, "full and partial RAX identities collapsed");
_Static_assert(PB_REG_LAST <= PB_REGSET_MAX_REG_ID, "REG enum exceeds PbRegSet storage");
_Static_assert(PB_REG_ID_COUNT > 400u, "Windows x64 REG member coverage is incomplete");
_Static_assert(sizeof(PbIargType) == 4, "PbIargType must be 32-bit");
_Static_assert(sizeof(PbCallOrder) == 4, "PbCallOrder must be 32-bit");
_Static_assert(sizeof(PbProcessorState) == 4, "PbProcessorState must be 32-bit");
_Static_assert(sizeof(PbSecType) == 4, "PbSecType must be 32-bit");
_Static_assert(sizeof(PbThreadId) == 4, "PbThreadId must be 32-bit");
_Static_assert(sizeof(PbPinConfigurationHandle) == sizeof(void*),
    "PbPinConfigurationHandle must be pointer-sized");
_Static_assert(sizeof(PbExceptionInfoHandle) == sizeof(void*),
    "PbExceptionInfoHandle must be pointer-sized");
_Static_assert(sizeof(PbXedDecodedInstHandle) == sizeof(void*),
    "PbXedDecodedInstHandle must be pointer-sized");
_Static_assert(sizeof(PbPhysicalContextHandle) == sizeof(void*),
    "PbPhysicalContextHandle must be pointer-sized");
_Static_assert(sizeof(PbConstPhysicalContextHandle) == sizeof(void*),
    "PbConstPhysicalContextHandle must be pointer-sized");
_Static_assert(sizeof(PbChildProcessHandle) == sizeof(void*),
    "PbChildProcessHandle must be pointer-sized");
_Static_assert(PB_ABI_VERSION_MAJOR == 1u && PB_ABI_VERSION_MINOR == 5u,
    "public ABI version must match v1.5");
_Static_assert(sizeof(PbLogType) == 4, "PbLogType must be 32-bit");
_Static_assert(sizeof(PbMessageKind) == 4, "PbMessageKind must be 32-bit");
_Static_assert(sizeof(PbMessageCallback) == sizeof(void*),
    "PbMessageCallback must be pointer-sized");
_Static_assert(sizeof(PbLockHandle) == sizeof(void*),
    "PbLockHandle must be pointer-sized");
_Static_assert(sizeof(PbMutexHandle) == sizeof(void*),
    "PbMutexHandle must be pointer-sized");
_Static_assert(sizeof(PbRwMutexHandle) == sizeof(void*),
    "PbRwMutexHandle must be pointer-sized");
_Static_assert(sizeof(PbSemaphoreHandle) == sizeof(void*),
    "PbSemaphoreHandle must be pointer-sized");
_Static_assert(sizeof(PbIargDescriptor) == 24,
    "PbIargDescriptor must have a fixed layout");
_Static_assert(PB_INST_ARGS_ENUM_CONSTANT_COUNT == 75u,
    "INST_ARGS enum count changed");
_Static_assert(sizeof(PbIpoint) == 4, "PbIpoint must be 32-bit");
_Static_assert(sizeof(PbBufferId) == 4, "PbBufferId must be 32-bit");
_Static_assert(PB_BUFFER_ID_INVALID == 0, "invalid buffer ID changed");
_Static_assert(sizeof(PbPinErrorType) == 4, "PbPinErrorType must be 32-bit");
_Static_assert(sizeof(PbPinErrorSeverity) == 4,
    "PbPinErrorSeverity must be 32-bit");
_Static_assert(sizeof(PbDetachProbedCallback) == sizeof(void*),
    "PbDetachProbedCallback must be pointer-sized");
_Static_assert(sizeof(PbOutOfMemoryCallback) == sizeof(void*),
    "PbOutOfMemoryCallback must be pointer-sized");
_Static_assert(sizeof(PbAttachProbedCallback) == sizeof(void*),
    "PbAttachProbedCallback must be pointer-sized");
_Static_assert(sizeof(PbFollowChildProcessCallback) == sizeof(void*),
    "PbFollowChildProcessCallback must be pointer-sized");
_Static_assert(sizeof(PbProbedCallCallback) == sizeof(void*),
    "PbProbedCallCallback must be pointer-sized");
_Static_assert(sizeof(PbBblAnalysisCallback) == sizeof(void*),
    "PbBblAnalysisCallback must be pointer-sized");
_Static_assert(sizeof(PbBblPredicateCallback) == sizeof(void*),
    "PbBblPredicateCallback must be pointer-sized");
_Static_assert(sizeof(PbAttachStatus) == 4, "PbAttachStatus must be 32-bit");
_Static_assert(sizeof(PbContextChangeReason) == 4, "PbContextChangeReason must be 32-bit");
_Static_assert(sizeof(PbExceptHandlingResult) == 4, "PbExceptHandlingResult must be 32-bit");
_Static_assert(sizeof(PbForkPoint) == 4, "PbForkPoint must be 32-bit");
_Static_assert(sizeof(PbCallbackType) == 4, "PbCallbackType must be 32-bit");
_Static_assert(sizeof(PbReplayMode) == 4, "PbReplayMode must be 32-bit");
_Static_assert(sizeof(PbSmcMode) == 4, "PbSmcMode must be 32-bit");
_Static_assert(sizeof(PbSymbolInfoMode) == 4, "PbSymbolInfoMode must be 32-bit");
_Static_assert(PB_ATTACH_INITIATED == 0u, "ATTACH_STATUS value changed");
_Static_assert(PB_CONTEXT_CHANGE_REASON_CALLBACK == 5u, "CONTEXT_CHANGE_REASON value changed");
_Static_assert(PB_EHR_CONTINUE_SEARCH == 2u, "EXCEPT_HANDLING_RESULT value changed");
_Static_assert(PB_FPOINT_AFTER_IN_CHILD == 2u, "FPOINT value changed");
_Static_assert(PB_PIN_CALLBACK_TYPE_SYSCALL == 1u, "PIN_CALLBACK_TYPE value changed");
_Static_assert(PB_REPLAY_MODE_ALL == 1u, "REPLAY_MODE alias changed");
_Static_assert(PB_SMC_DISABLE == 1u, "SMC mode value changed");
_Static_assert(PB_DEBUG_OR_EXPORT_SYMBOLS == 3u, "SYMBOL_INFO_MODE mask changed");
_Static_assert(PB_SEC_TYPE_COUNT == 27u, "SEC_TYPE coverage is incomplete");
_Static_assert(PB_FXSAVE_SIZE == 512u, "PbFxSave size constant changed");
_Static_assert(sizeof(PbFxSave) == 512, "PbFxSave must be 512 bytes");
_Static_assert(offsetof(PbFxSave, bytes) == 0, "PbFxSave layout changed");
_Static_assert(sizeof(PbMemoryTransInfo) == 40,
    "PbMemoryTransInfo must be 40 bytes");
_Static_assert(sizeof(PbMemRange) == 16, "PbMemRange must be 16 bytes");
_Static_assert(offsetof(PbMemRange, base) == 0,
    "PbMemRange base offset changed");
_Static_assert(offsetof(PbMemRange, size) == 8,
    "PbMemRange size offset changed");
_Static_assert(offsetof(PbMemoryTransInfo, address) == 0,
    "PbMemoryTransInfo address offset changed");
_Static_assert(offsetof(PbMemoryTransInfo, thread_id) == 24,
    "PbMemoryTransInfo thread offset changed");
_Static_assert(offsetof(PbMemoryTransInfo, reserved) == 36,
    "PbMemoryTransInfo tail layout changed");
_Static_assert(sizeof(PbExceptionInfoSnapshot) == 88,
    "PbExceptionInfoSnapshot must be 88 bytes");
_Static_assert(offsetof(PbExceptionInfoSnapshot, exception_code) == 0,
    "PbExceptionInfoSnapshot code offset changed");
_Static_assert(offsetof(PbExceptionInfoSnapshot, faulty_access_address) == 24,
    "PbExceptionInfoSnapshot fault address offset changed");
_Static_assert(offsetof(PbExceptionInfoSnapshot, windows_arguments) == 48,
    "PbExceptionInfoSnapshot argument offset changed");
_Static_assert(PB_EXCEPTION_INFO_HAS_FAULT_ADDRESS == 1u,
    "exception snapshot fault-address flag changed");
_Static_assert(PB_EXCEPTION_INFO_HAS_FP_ERRORS == 2u,
    "exception snapshot FP flag changed");
_Static_assert(PB_EXCEPTION_INFO_HAS_WINDOWS_DETAILS == 4u,
    "exception snapshot Windows flag changed");
_Static_assert(PB_REGSET_WORD_COUNT == 16u, "PbRegSet word count changed");
_Static_assert(sizeof(PbRegSet) == 128, "PbRegSet must be 128 bytes");
_Static_assert(offsetof(PbRegSet, words) == 0, "PbRegSet layout changed");
_Static_assert(offsetof(PbInsHandle, opaque) == 0, "PbInsHandle layout changed");
_Static_assert(offsetof(PbRtnHandle, opaque) == 0, "PbRtnHandle layout changed");
_Static_assert(offsetof(PbBblHandle, opaque) == 0, "PbBblHandle layout changed");
_Static_assert(offsetof(PbSecHandle, opaque) == 0, "PbSecHandle layout changed");
_Static_assert(offsetof(PbImgHandle, opaque) == 0, "PbImgHandle layout changed");
_Static_assert(offsetof(PbSymHandle, opaque) == 0, "PbSymHandle layout changed");
_Static_assert(offsetof(PbCallbackHandle, opaque) == 0, "PbCallbackHandle layout changed");

int main(void)
{
    PbLockHandle lock = 0;
    PbMutexHandle mutex = 0;
    PbRwMutexHandle rwmutex = 0;
    PbSemaphoreHandle semaphore = 0;
    (void)lock;
    (void)mutex;
    (void)rwmutex;
    (void)semaphore;
    if (PB_ABI_VERSION != ((1u << 16u) | 5u))
        return 1;
    if (PB_OK != 0 || PB_ERR_INVALID_ARGUMENT == PB_OK)
        return 2;
    return 0;
}
