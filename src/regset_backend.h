#ifndef PINBRIDGE_REGSET_BACKEND_H
#define PINBRIDGE_REGSET_BACKEND_H

#include "pinbridge/pinbridge.h"

PbStatus PbBackendRegSetAddAll(PbRegSet* set);
PbStatus PbBackendRegSetClear(PbRegSet* set);
PbStatus PbBackendRegSetContains(const PbRegSet* set, PbRegId reg, uint8_t* out_contains);
PbStatus PbBackendRegSetInsert(PbRegSet* set, PbRegId reg);
PbStatus PbBackendRegSetPopCount(const PbRegSet* set, uint32_t* out_count);
PbStatus PbBackendRegSetIsEmpty(const PbRegSet* set, uint8_t* out_is_empty);
PbStatus PbBackendRegSetPopNext(PbRegSet* set, PbRegId* out_reg);
PbStatus PbBackendRegSetRemove(PbRegSet* set, PbRegId reg);
PbStatus PbBackendRegSetFirst(PbRegId* out_reg);
PbStatus PbBackendRegSetLast(PbRegId* out_reg);
PbStatus PbBackendRegSetStringShort(
    const PbRegSet* set, char* buffer, uint64_t capacity, uint64_t* required_size);

#endif
