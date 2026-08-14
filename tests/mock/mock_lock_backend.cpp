#include "lock_backend.h"

#include <cstdlib>

namespace
{
struct MockSync { int32_t state; int32_t owner; };
template <typename Handle> MockSync* As(Handle handle) { return reinterpret_cast<MockSync*>(handle); }
template <typename Handle> PbStatus Create(Handle* out) { MockSync* value = static_cast<MockSync*>(std::calloc(1, sizeof(MockSync))); if (!value) return PB_ERR_OUT_OF_MEMORY; *out = reinterpret_cast<Handle>(value); return PB_OK; }
template <typename Handle> PbStatus Destroy(Handle handle) { std::free(As(handle)); return PB_OK; }
}

PbStatus PbBackendLockInit(PbLockHandle* out) { return Create(out); }
PbStatus PbBackendLockDestroy(PbLockHandle h) { return Destroy(h); }
PbStatus PbBackendLockGet(PbLockHandle h, int32_t value) { As(h)->state = 1; As(h)->owner = value; return PB_OK; }
PbStatus PbBackendLockRelease(PbLockHandle h, int32_t* owner) { *owner = As(h)->owner; As(h)->state = 0; return PB_OK; }
PbStatus PbBackendMutexInit(PbMutexHandle* out) { return Create(out); }
PbStatus PbBackendMutexFini(PbMutexHandle h) { return Destroy(h); }
PbStatus PbBackendMutexLock(PbMutexHandle h) { As(h)->state = 1; return PB_OK; }
PbStatus PbBackendMutexTryLock(PbMutexHandle h, uint8_t* acquired) { *acquired = As(h)->state ? 0 : 1; if (*acquired) As(h)->state = 1; return PB_OK; }
PbStatus PbBackendMutexUnlock(PbMutexHandle h) { As(h)->state = 0; return PB_OK; }
PbStatus PbBackendRwMutexInit(PbRwMutexHandle* out) { return Create(out); }
PbStatus PbBackendRwMutexFini(PbRwMutexHandle h) { return Destroy(h); }
PbStatus PbBackendRwMutexReadLock(PbRwMutexHandle h) { ++As(h)->state; return PB_OK; }
PbStatus PbBackendRwMutexTryReadLock(PbRwMutexHandle h, uint8_t* acquired) { *acquired = As(h)->state < 0 ? 0 : 1; if (*acquired) ++As(h)->state; return PB_OK; }
PbStatus PbBackendRwMutexTryWriteLock(PbRwMutexHandle h, uint8_t* acquired) { *acquired = As(h)->state == 0 ? 1 : 0; if (*acquired) As(h)->state = -1; return PB_OK; }
PbStatus PbBackendRwMutexUnlock(PbRwMutexHandle h) { if (As(h)->state > 0) --As(h)->state; else As(h)->state = 0; return PB_OK; }
PbStatus PbBackendRwMutexWriteLock(PbRwMutexHandle h) { As(h)->state = -1; return PB_OK; }
PbStatus PbBackendSemaphoreInit(PbSemaphoreHandle* out) { return Create(out); }
PbStatus PbBackendSemaphoreFini(PbSemaphoreHandle h) { return Destroy(h); }
PbStatus PbBackendSemaphoreClear(PbSemaphoreHandle h) { As(h)->state = 0; return PB_OK; }
PbStatus PbBackendSemaphoreIsSet(PbSemaphoreHandle h, uint8_t* is_set) { *is_set = As(h)->state ? 1 : 0; return PB_OK; }
PbStatus PbBackendSemaphoreSet(PbSemaphoreHandle h) { As(h)->state = 1; return PB_OK; }
PbStatus PbBackendSemaphoreTimedWait(PbSemaphoreHandle h, uint32_t, uint8_t* is_set) { return PbBackendSemaphoreIsSet(h, is_set); }
PbStatus PbBackendSemaphoreWait(PbSemaphoreHandle h) { return As(h)->state ? PB_OK : PB_ERR_INVALID_STATE; }
