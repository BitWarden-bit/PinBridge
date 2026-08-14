#include "pin.H"
#include "knobs_backend.h"
#include <cstring>
namespace { KNOB_BASE* K(PbKnobHandle h){return reinterpret_cast<KNOB_BASE*>(h);} PbKnobHandle H(KNOB_BASE*k){return reinterpret_cast<PbKnobHandle>(k);} PbStatus Copy(const std::string&s,char*b,uint64_t c,uint64_t*r){*r=s.size()+1;if(!b||c<*r)return PB_ERR_BUFFER_TOO_SMALL;std::memcpy(b,s.c_str(),static_cast<size_t>(*r));return PB_OK;} }
static_assert(PB_KNOB_MODE_INVALID==KNOB_MODE_INVALID,"knob enum"); static_assert(PB_KNOB_MODE_LAST==KNOB_MODE_LAST,"knob enum");
PbStatus PbBackendKnobCheckAll(uint8_t v){KNOB_BASE::CheckAllKnobs(v!=0);return PB_OK;}
PbStatus PbBackendKnobCompare(PbKnobHandle a,PbKnobHandle b,int32_t*o){*o=K(a)->Compare(*K(b));return PB_OK;}
PbStatus PbBackendKnobFind(const char*s,uint32_t kind,PbKnobHandle*o){KNOB_BASE*k=kind==0?KNOB_BASE::FindEnabledKnob(s):(kind==1?KNOB_BASE::FindFamily(s):KNOB_BASE::FindKnob(s));*o=H(k);return PB_OK;}
PbStatus PbBackendKnobSlowAsserts(uint8_t*o){*o=KnobSlowAsserts.Value()?1u:0u;return PB_OK;} PbStatus PbBackendKnobCount(uint32_t*o){*o=KNOB_BASE::NumberOfKnobs();return PB_OK;}
PbStatus PbBackendKnobSetByUser(PbKnobHandle k,uint8_t*o){*o=K(k)->SetByUser()?1u:0u;return PB_OK;}
PbStatus PbBackendKnobString(uint32_t k,char*b,uint64_t c,uint64_t*r){return Copy(k?KNOB_BASE::StringLongAll():KNOB_BASE::StringKnobSummary(),b,c,r);}
PbStatus PbBackendKnobTurnOnSetByUser(PbKnobHandle k){K(k)->TurnOnSetByUser();return PB_OK;}
