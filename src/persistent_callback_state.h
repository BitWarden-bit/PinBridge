#ifndef PINBRIDGE_PERSISTENT_CALLBACK_STATE_H
#define PINBRIDGE_PERSISTENT_CALLBACK_STATE_H

#include <cstdlib>

// Pin instrumentation callbacks are serialized by the VM lock. Generated
// code retains the descriptor pointer across trace invalidation, so entries
// intentionally live until process exit. Interning by callback/user pair
// makes that storage proportional to consumers rather than insertion sites.
template< typename Callback >
struct PbPersistentCallbackState
{
    Callback callback;
    void* user_data;
    PbPersistentCallbackState* next;
};

template< typename Callback >
PbPersistentCallbackState<Callback>* PbInternPersistentCallbackState(
    PbPersistentCallbackState<Callback>*& head,
    Callback callback, void* user_data)
{
    for (PbPersistentCallbackState<Callback>* state = head;
         state; state = state->next)
    {
        if (state->callback == callback && state->user_data == user_data)
            return state;
    }
    PbPersistentCallbackState<Callback>* state =
        static_cast<PbPersistentCallbackState<Callback>*>(
            std::malloc(sizeof(PbPersistentCallbackState<Callback>)));
    if (!state)
        return 0;
    state->callback = callback;
    state->user_data = user_data;
    state->next = head;
    head = state;
    return state;
}

#endif
