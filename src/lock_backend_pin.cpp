#include "pin.H"
#include "lock_backend.h"

#include <cstdlib>
#include <malloc.h>

namespace
{
template <typename Native, typename Handle>
Native* As(Handle handle)
{
    return reinterpret_cast<Native*>(handle);
}

template <typename Native, typename Handle>
PbStatus Allocate(Handle* out_handle)
{
    Native* value = static_cast<Native*>(memalign(64, sizeof(Native)));
    if (!value) return PB_ERR_OUT_OF_MEMORY;
    *out_handle = reinterpret_cast<Handle>(value);
    return PB_OK;
}
}

PbStatus PbBackendLockInit(PbLockHandle* out_lock)
{
    PbStatus status = Allocate<PIN_LOCK>(out_lock);
    if (status == PB_OK) PIN_InitLock(As<PIN_LOCK>(*out_lock));
    return status;
}
PbStatus PbBackendLockDestroy(PbLockHandle lock) { std::free(As<PIN_LOCK>(lock)); return PB_OK; }
PbStatus PbBackendLockGet(PbLockHandle lock, int32_t value) { PIN_GetLock(As<PIN_LOCK>(lock), value); return PB_OK; }
PbStatus PbBackendLockRelease(PbLockHandle lock, int32_t* out_owner) { *out_owner = PIN_ReleaseLock(As<PIN_LOCK>(lock)); return PB_OK; }

PbStatus PbBackendMutexInit(PbMutexHandle* out_mutex)
{
    PbStatus status = Allocate<PIN_MUTEX>(out_mutex);
    if (status != PB_OK) return status;
    if (PIN_MutexInit(As<PIN_MUTEX>(*out_mutex))) return PB_OK;
    std::free(As<PIN_MUTEX>(*out_mutex)); *out_mutex = 0; return PB_ERR_INTERNAL;
}
PbStatus PbBackendMutexFini(PbMutexHandle mutex) { PIN_MutexFini(As<PIN_MUTEX>(mutex)); std::free(As<PIN_MUTEX>(mutex)); return PB_OK; }
PbStatus PbBackendMutexLock(PbMutexHandle mutex) { PIN_MutexLock(As<PIN_MUTEX>(mutex)); return PB_OK; }
PbStatus PbBackendMutexTryLock(PbMutexHandle mutex, uint8_t* out_acquired) { *out_acquired = PIN_MutexTryLock(As<PIN_MUTEX>(mutex)) ? 1 : 0; return PB_OK; }
PbStatus PbBackendMutexUnlock(PbMutexHandle mutex) { PIN_MutexUnlock(As<PIN_MUTEX>(mutex)); return PB_OK; }

PbStatus PbBackendRwMutexInit(PbRwMutexHandle* out_mutex)
{
    PbStatus status = Allocate<PIN_RWMUTEX>(out_mutex);
    if (status != PB_OK) return status;
    if (PIN_RWMutexInit(As<PIN_RWMUTEX>(*out_mutex))) return PB_OK;
    std::free(As<PIN_RWMUTEX>(*out_mutex)); *out_mutex = 0; return PB_ERR_INTERNAL;
}
PbStatus PbBackendRwMutexFini(PbRwMutexHandle mutex) { PIN_RWMutexFini(As<PIN_RWMUTEX>(mutex)); std::free(As<PIN_RWMUTEX>(mutex)); return PB_OK; }
PbStatus PbBackendRwMutexReadLock(PbRwMutexHandle mutex) { PIN_RWMutexReadLock(As<PIN_RWMUTEX>(mutex)); return PB_OK; }
PbStatus PbBackendRwMutexTryReadLock(PbRwMutexHandle mutex, uint8_t* out_acquired) { *out_acquired = PIN_RWMutexTryReadLock(As<PIN_RWMUTEX>(mutex)) ? 1 : 0; return PB_OK; }
PbStatus PbBackendRwMutexTryWriteLock(PbRwMutexHandle mutex, uint8_t* out_acquired) { *out_acquired = PIN_RWMutexTryWriteLock(As<PIN_RWMUTEX>(mutex)) ? 1 : 0; return PB_OK; }
PbStatus PbBackendRwMutexUnlock(PbRwMutexHandle mutex) { PIN_RWMutexUnlock(As<PIN_RWMUTEX>(mutex)); return PB_OK; }
PbStatus PbBackendRwMutexWriteLock(PbRwMutexHandle mutex) { PIN_RWMutexWriteLock(As<PIN_RWMUTEX>(mutex)); return PB_OK; }

PbStatus PbBackendSemaphoreInit(PbSemaphoreHandle* out_semaphore)
{
    PbStatus status = Allocate<PIN_SEMAPHORE>(out_semaphore);
    if (status != PB_OK) return status;
    if (PIN_SemaphoreInit(As<PIN_SEMAPHORE>(*out_semaphore))) return PB_OK;
    std::free(As<PIN_SEMAPHORE>(*out_semaphore)); *out_semaphore = 0; return PB_ERR_INTERNAL;
}
PbStatus PbBackendSemaphoreFini(PbSemaphoreHandle semaphore) { PIN_SemaphoreFini(As<PIN_SEMAPHORE>(semaphore)); std::free(As<PIN_SEMAPHORE>(semaphore)); return PB_OK; }
PbStatus PbBackendSemaphoreClear(PbSemaphoreHandle semaphore) { PIN_SemaphoreClear(As<PIN_SEMAPHORE>(semaphore)); return PB_OK; }
PbStatus PbBackendSemaphoreIsSet(PbSemaphoreHandle semaphore, uint8_t* out_is_set) { *out_is_set = PIN_SemaphoreIsSet(As<PIN_SEMAPHORE>(semaphore)) ? 1 : 0; return PB_OK; }
PbStatus PbBackendSemaphoreSet(PbSemaphoreHandle semaphore) { PIN_SemaphoreSet(As<PIN_SEMAPHORE>(semaphore)); return PB_OK; }
PbStatus PbBackendSemaphoreTimedWait(PbSemaphoreHandle semaphore, uint32_t timeout_ms, uint8_t* out_is_set) { *out_is_set = PIN_SemaphoreTimedWait(As<PIN_SEMAPHORE>(semaphore), timeout_ms) ? 1 : 0; return PB_OK; }
PbStatus PbBackendSemaphoreWait(PbSemaphoreHandle semaphore) { PIN_SemaphoreWait(As<PIN_SEMAPHORE>(semaphore)); return PB_OK; }
