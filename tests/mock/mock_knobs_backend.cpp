#include "knobs_backend.h"
#include <cstring>
#include <string>
namespace { struct K{bool enabled;bool user;const char*name;const char*family;}; K a={true,false,"alpha","bridge"}; K b={true,false,"beta","bridge"}; PbKnobHandle H(K*k){return reinterpret_cast<PbKnobHandle>(k);} K* P(PbKnobHandle h){return reinterpret_cast<K*>(h);} PbStatus Copy(const std::string&s,char*bfr,uint64_t c,uint64_t*r){*r=s.size()+1;if(!bfr||c<*r)return PB_ERR_BUFFER_TOO_SMALL;std::memcpy(bfr,s.c_str(),static_cast<size_t>(*r));return PB_OK;} }
PbStatus PbBackendKnobCheckAll(uint8_t){return PB_OK;} PbStatus PbBackendKnobCompare(PbKnobHandle x,PbKnobHandle y,int32_t*o){*o=std::strcmp(P(x)->name,P(y)->name);return PB_OK;}
PbStatus PbBackendKnobFind(const char*s,uint32_t kind,PbKnobHandle*o){*o=0;for(K*k:{&a,&b})if((kind==1?std::strcmp(k->family,s):std::strcmp(k->name,s))==0&&(kind!=0||k->enabled)){*o=H(k);break;}return PB_OK;}
PbStatus PbBackendKnobSlowAsserts(uint8_t*o){*o=0;return PB_OK;} PbStatus PbBackendKnobCount(uint32_t*o){*o=2;return PB_OK;} PbStatus PbBackendKnobSetByUser(PbKnobHandle k,uint8_t*o){*o=P(k)->user?1:0;return PB_OK;}
PbStatus PbBackendKnobString(uint32_t kind,char*bfr,uint64_t c,uint64_t*r){return Copy(kind?"alpha beta long":"alpha beta",bfr,c,r);} PbStatus PbBackendKnobTurnOnSetByUser(PbKnobHandle k){P(k)->user=true;return PB_OK;}
