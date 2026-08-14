#include "pin.H"
#include "message_backend.h"
#include <cstring>
#include <string>
namespace{PbStatus Copy(const std::string&s,char*b,uint64_t c,uint64_t*r){*r=s.size()+1;if(!b||c<*r)return PB_ERR_BUFFER_TOO_SMALL;std::memcpy(b,s.c_str(),static_cast<size_t>(*r));return PB_OK;}}
PbStatus PbBackendAssertString(const char*f,const char*n,uint32_t l,const char*m,char*b,uint64_t c,uint64_t*r){return Copy(AssertString(f,n,l,m),b,c,r);}
PbStatus PbBackendBreakMe(void){BreakMe();return PB_OK;}
PbStatus PbBackendMillisecondsElapsed(uint64_t*o){*o=MilliSecondsElapsed();return PB_OK;}
