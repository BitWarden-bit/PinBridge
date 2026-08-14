#include "pinbridge/pinbridge.h"
#include "message_backend.h"
namespace{template<class F>PbStatus G(F f){
#if defined(_CPPUNWIND)||defined(__cpp_exceptions)
try{return f();}catch(...){return PB_ERR_INTERNAL;}
#else
return f();
#endif
}}
PbStatus PB_CALL pb_assert_string(const char*f,const char*n,uint32_t l,const char*m,char*b,uint64_t c,uint64_t*r){if(r)*r=0;if(!f||!n||!m||!r||(!b&&c))return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendAssertString(f,n,l,m,b,c,r);});}
PbStatus PB_CALL pb_break_me(void){return G([](){return PbBackendBreakMe();});}
PbStatus PB_CALL pb_milliseconds_elapsed(uint64_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendMillisecondsElapsed(o);});}
