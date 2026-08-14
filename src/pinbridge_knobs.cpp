#include "pinbridge/pinbridge.h"
#include "knobs_backend.h"
namespace { template<class F> PbStatus Guard(F f) {
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
 try{return f();}catch(...){return PB_ERR_INTERNAL;}
#else
 return f();
#endif
} }
PbStatus PB_CALL pb_knob_check_all(uint8_t v){if(v>1)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobCheckAll(v);});}
PbStatus PB_CALL pb_knob_compare(PbKnobHandle a,PbKnobHandle b,int32_t* o){if(o)*o=0;if(!a||!b||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobCompare(a,b,o);});}
static PbStatus Find(const char*s,uint32_t k,PbKnobHandle*o){if(o)*o=PB_KNOB_HANDLE_INVALID;if(!s||!*s||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobFind(s,k,o);});}
PbStatus PB_CALL pb_knob_find_enabled(const char*s,PbKnobHandle*o){return Find(s,0,o);} PbStatus PB_CALL pb_knob_find_family(const char*s,PbKnobHandle*o){return Find(s,1,o);} PbStatus PB_CALL pb_knob_find(const char*s,PbKnobHandle*o){return Find(s,2,o);}
PbStatus PB_CALL pb_knob_slow_asserts(uint8_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobSlowAsserts(o);});}
PbStatus PB_CALL pb_knob_count(uint32_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobCount(o);});}
PbStatus PB_CALL pb_knob_set_by_user(PbKnobHandle k,uint8_t*o){if(o)*o=0;if(!k||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobSetByUser(k,o);});}
static PbStatus Str(uint32_t k,char*b,uint64_t c,uint64_t*r){if(r)*r=0;if(!r||(!b&&c))return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobString(k,b,c,r);});}
PbStatus PB_CALL pb_knob_summary(char*b,uint64_t c,uint64_t*r){return Str(0,b,c,r);} PbStatus PB_CALL pb_knob_long_all(char*b,uint64_t c,uint64_t*r){return Str(1,b,c,r);}
PbStatus PB_CALL pb_knob_turn_on_set_by_user(PbKnobHandle k){if(!k)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendKnobTurnOnSetByUser(k);});}
