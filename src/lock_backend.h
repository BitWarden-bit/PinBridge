#ifndef PINBRIDGE_LOCK_BACKEND_H
#define PINBRIDGE_LOCK_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendLockInit(PbLockHandle* out_lock);
PbStatus PbBackendLockDestroy(PbLockHandle lock);
PbStatus PbBackendLockGet(PbLockHandle lock, int32_t value);
PbStatus PbBackendLockRelease(PbLockHandle lock, int32_t* out_owner);
PbStatus PbBackendMutexInit(PbMutexHandle* out_mutex);
PbStatus PbBackendMutexFini(PbMutexHandle mutex);
PbStatus PbBackendMutexLock(PbMutexHandle mutex);
PbStatus PbBackendMutexTryLock(PbMutexHandle mutex, uint8_t* out_acquired);
PbStatus PbBackendMutexUnlock(PbMutexHandle mutex);
PbStatus PbBackendRwMutexInit(PbRwMutexHandle* out_mutex);
PbStatus PbBackendRwMutexFini(PbRwMutexHandle mutex);
PbStatus PbBackendRwMutexReadLock(PbRwMutexHandle mutex);
PbStatus PbBackendRwMutexTryReadLock(PbRwMutexHandle mutex, uint8_t* out_acquired);
PbStatus PbBackendRwMutexTryWriteLock(PbRwMutexHandle mutex, uint8_t* out_acquired);
PbStatus PbBackendRwMutexUnlock(PbRwMutexHandle mutex);
PbStatus PbBackendRwMutexWriteLock(PbRwMutexHandle mutex);
PbStatus PbBackendSemaphoreInit(PbSemaphoreHandle* out_semaphore);
PbStatus PbBackendSemaphoreFini(PbSemaphoreHandle semaphore);
PbStatus PbBackendSemaphoreClear(PbSemaphoreHandle semaphore);
PbStatus PbBackendSemaphoreIsSet(PbSemaphoreHandle semaphore, uint8_t* out_is_set);
PbStatus PbBackendSemaphoreSet(PbSemaphoreHandle semaphore);
PbStatus PbBackendSemaphoreTimedWait(
    PbSemaphoreHandle semaphore, uint32_t timeout_ms, uint8_t* out_is_set);
PbStatus PbBackendSemaphoreWait(PbSemaphoreHandle semaphore);

#endif
