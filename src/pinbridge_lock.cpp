#include "pinbridge/pinbridge.h"
#include "lock_backend.h"

namespace
{
template <typename Function>
PbStatus Guard(Function function)
{
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
    try
    {
        return function();
    }
    catch (...)
    {
        return PB_ERR_INTERNAL;
    }
#else
    return function();
#endif
}
}

#define PB_HANDLE_CALL(name, backend, type) \
    PbStatus PB_CALL name(type handle) \
    { \
        if (!handle) return PB_ERR_INVALID_ARGUMENT; \
        return Guard([&]() { return backend(handle); }); \
    }

PbStatus PB_CALL pb_pin_init_lock(PbLockHandle* out_lock)
{
    if (out_lock) *out_lock = 0;
    if (!out_lock) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendLockInit(out_lock); });
}
PB_HANDLE_CALL(pb_pin_lock_destroy, PbBackendLockDestroy, PbLockHandle)
PbStatus PB_CALL pb_pin_get_lock(PbLockHandle lock, int32_t value)
{
    if (!lock) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendLockGet(lock, value); });
}
PbStatus PB_CALL pb_pin_release_lock(PbLockHandle lock, int32_t* out_owner)
{
    if (out_owner) *out_owner = 0;
    if (!lock || !out_owner) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendLockRelease(lock, out_owner); });
}

#define PB_INIT_CALL(name, backend, type) \
    PbStatus PB_CALL name(type* out_handle) \
    { \
        if (out_handle) *out_handle = 0; \
        if (!out_handle) return PB_ERR_INVALID_ARGUMENT; \
        return Guard([&]() { return backend(out_handle); }); \
    }

PB_INIT_CALL(pb_pin_mutex_init, PbBackendMutexInit, PbMutexHandle)
PB_HANDLE_CALL(pb_pin_mutex_fini, PbBackendMutexFini, PbMutexHandle)
PB_HANDLE_CALL(pb_pin_mutex_lock, PbBackendMutexLock, PbMutexHandle)
PbStatus PB_CALL pb_pin_mutex_try_lock(PbMutexHandle mutex, uint8_t* out_acquired)
{
    if (out_acquired) *out_acquired = 0;
    if (!mutex || !out_acquired) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendMutexTryLock(mutex, out_acquired); });
}
PB_HANDLE_CALL(pb_pin_mutex_unlock, PbBackendMutexUnlock, PbMutexHandle)

PB_INIT_CALL(pb_pin_rwmutex_init, PbBackendRwMutexInit, PbRwMutexHandle)
PB_HANDLE_CALL(pb_pin_rwmutex_fini, PbBackendRwMutexFini, PbRwMutexHandle)
PB_HANDLE_CALL(pb_pin_rwmutex_read_lock, PbBackendRwMutexReadLock, PbRwMutexHandle)
PbStatus PB_CALL pb_pin_rwmutex_try_read_lock(
    PbRwMutexHandle mutex, uint8_t* out_acquired)
{
    if (out_acquired) *out_acquired = 0;
    if (!mutex || !out_acquired) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendRwMutexTryReadLock(mutex, out_acquired); });
}
PbStatus PB_CALL pb_pin_rwmutex_try_write_lock(
    PbRwMutexHandle mutex, uint8_t* out_acquired)
{
    if (out_acquired) *out_acquired = 0;
    if (!mutex || !out_acquired) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendRwMutexTryWriteLock(mutex, out_acquired); });
}
PB_HANDLE_CALL(pb_pin_rwmutex_unlock, PbBackendRwMutexUnlock, PbRwMutexHandle)
PB_HANDLE_CALL(pb_pin_rwmutex_write_lock, PbBackendRwMutexWriteLock, PbRwMutexHandle)

PB_INIT_CALL(pb_pin_semaphore_init, PbBackendSemaphoreInit, PbSemaphoreHandle)
PB_HANDLE_CALL(pb_pin_semaphore_fini, PbBackendSemaphoreFini, PbSemaphoreHandle)
PB_HANDLE_CALL(pb_pin_semaphore_clear, PbBackendSemaphoreClear, PbSemaphoreHandle)
PbStatus PB_CALL pb_pin_semaphore_is_set(
    PbSemaphoreHandle semaphore, uint8_t* out_is_set)
{
    if (out_is_set) *out_is_set = 0;
    if (!semaphore || !out_is_set) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() { return PbBackendSemaphoreIsSet(semaphore, out_is_set); });
}
PB_HANDLE_CALL(pb_pin_semaphore_set, PbBackendSemaphoreSet, PbSemaphoreHandle)
PbStatus PB_CALL pb_pin_semaphore_timed_wait(
    PbSemaphoreHandle semaphore, uint32_t timeout_ms, uint8_t* out_is_set)
{
    if (out_is_set) *out_is_set = 0;
    if (!semaphore || !out_is_set) return PB_ERR_INVALID_ARGUMENT;
    return Guard([&]() {
        return PbBackendSemaphoreTimedWait(semaphore, timeout_ms, out_is_set);
    });
}
PB_HANDLE_CALL(pb_pin_semaphore_wait, PbBackendSemaphoreWait, PbSemaphoreHandle)

#undef PB_INIT_CALL
#undef PB_HANDLE_CALL
