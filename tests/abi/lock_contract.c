#include <stdint.h>
#include "pinbridge/pinbridge.h"

int main(void)
{
    PbLockHandle lock = 0;
    PbMutexHandle mutex = 0;
    PbRwMutexHandle rwmutex = 0;
    PbSemaphoreHandle semaphore = 0;
    int32_t owner = 0;
    uint8_t value = 0;
    if (pb_pin_init_lock(&lock) != PB_OK || !lock ||
        pb_pin_get_lock(lock, 47) != PB_OK ||
        pb_pin_release_lock(lock, &owner) != PB_OK || owner != 47 ||
        pb_pin_lock_destroy(lock) != PB_OK) return 1;
    if (pb_pin_mutex_init(&mutex) != PB_OK || !mutex ||
        pb_pin_mutex_try_lock(mutex, &value) != PB_OK || !value ||
        pb_pin_mutex_unlock(mutex) != PB_OK ||
        pb_pin_mutex_lock(mutex) != PB_OK || pb_pin_mutex_unlock(mutex) != PB_OK ||
        pb_pin_mutex_fini(mutex) != PB_OK) return 2;
    if (pb_pin_rwmutex_init(&rwmutex) != PB_OK || !rwmutex ||
        pb_pin_rwmutex_try_read_lock(rwmutex, &value) != PB_OK || !value ||
        pb_pin_rwmutex_unlock(rwmutex) != PB_OK ||
        pb_pin_rwmutex_read_lock(rwmutex) != PB_OK || pb_pin_rwmutex_unlock(rwmutex) != PB_OK ||
        pb_pin_rwmutex_try_write_lock(rwmutex, &value) != PB_OK || !value ||
        pb_pin_rwmutex_unlock(rwmutex) != PB_OK ||
        pb_pin_rwmutex_write_lock(rwmutex) != PB_OK || pb_pin_rwmutex_unlock(rwmutex) != PB_OK ||
        pb_pin_rwmutex_fini(rwmutex) != PB_OK) return 3;
    if (pb_pin_semaphore_init(&semaphore) != PB_OK || !semaphore ||
        pb_pin_semaphore_is_set(semaphore, &value) != PB_OK || value ||
        pb_pin_semaphore_timed_wait(semaphore, 0, &value) != PB_OK || value ||
        pb_pin_semaphore_set(semaphore) != PB_OK ||
        pb_pin_semaphore_is_set(semaphore, &value) != PB_OK || !value ||
        pb_pin_semaphore_wait(semaphore) != PB_OK ||
        pb_pin_semaphore_clear(semaphore) != PB_OK ||
        pb_pin_semaphore_fini(semaphore) != PB_OK) return 4;
    if (pb_pin_init_lock(0) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_mutex_try_lock(0, &value) != PB_ERR_INVALID_ARGUMENT ||
        pb_pin_semaphore_is_set(semaphore, 0) != PB_ERR_INVALID_ARGUMENT) return 5;
    return 0;
}
