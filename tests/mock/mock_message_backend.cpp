#include "message_backend.h"
#include <cstdio>
#include <cstring>
PbStatus PbBackendAssertString(const char*f,const char*n,uint32_t l,const char*m,char*b,uint64_t c,uint64_t*r){char s[256]={};int z=std::snprintf(s,sizeof(s),"%s:%u: %s: %s",f,l,n,m);if(z<0)return PB_ERR_INTERNAL;*r=static_cast<uint64_t>(z)+1;if(!b||c<*r)return PB_ERR_BUFFER_TOO_SMALL;std::memcpy(b,s,static_cast<size_t>(*r));return PB_OK;}
PbStatus PbBackendBreakMe(void){return PB_OK;}
PbStatus PbBackendMillisecondsElapsed(uint64_t*o){*o=0;return PB_OK;}
