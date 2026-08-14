#ifndef PINBRIDGE_KNOBS_BACKEND_H
#define PINBRIDGE_KNOBS_BACKEND_H
#include "pinbridge/pinbridge.h"
PbStatus PbBackendKnobCheckAll(uint8_t allow_dashes);
PbStatus PbBackendKnobCompare(PbKnobHandle left, PbKnobHandle right, int32_t* out);
PbStatus PbBackendKnobFind(const char* text, uint32_t kind, PbKnobHandle* out);
PbStatus PbBackendKnobSlowAsserts(uint8_t* out);
PbStatus PbBackendKnobCount(uint32_t* out);
PbStatus PbBackendKnobSetByUser(PbKnobHandle knob, uint8_t* out);
PbStatus PbBackendKnobString(uint32_t kind, char* buffer, uint64_t capacity, uint64_t* required);
PbStatus PbBackendKnobTurnOnSetByUser(PbKnobHandle knob);
#endif
