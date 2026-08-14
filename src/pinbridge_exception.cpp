#include "pinbridge/pinbridge.h"
#include "exception_backend.h"
namespace{template<class F>PbStatus G(F f){
#if defined(_CPPUNWIND)||defined(__cpp_exceptions)
try{return f();}catch(...){return PB_ERR_INTERNAL;}
#else
return f();
#endif
}bool Code(PbExceptionCode c){return c<=PB_EXCEPTCODE_RECEIVED_AMBIGUOUS_SIMD;}bool Type(PbFaultyAccessType t){return t<=PB_FAULTY_ACCESS_EXECUTE;}}
PbStatus PB_CALL pb_exception_info_release(PbExceptionInfoHandle h){if(!h)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionRelease(h);});}
PbStatus PB_CALL pb_pin_count_windows_exception_arguments(PbExceptionInfoHandle h,uint32_t*o){if(o)*o=0;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionCount(h,o);});}
PbStatus PB_CALL pb_pin_exception_to_string(PbExceptionInfoHandle h,char*b,uint64_t c,uint64_t*r){if(r)*r=0;if(!h||!r||(!b&&c))return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionString(h,b,c,r);});}
PbStatus PB_CALL pb_pin_get_exception_address(PbExceptionInfoHandle h,uint64_t*o){if(o)*o=0;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionAddress(h,o);});}
PbStatus PB_CALL pb_pin_get_exception_class(PbExceptionCode c,PbExceptionClass*o){if(o)*o=PB_EXCEPTCLASS_NONE;if(!Code(c)||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionClass(c,o);});}
PbStatus PB_CALL pb_pin_get_exception_code(PbExceptionInfoHandle h,PbExceptionCode*o){if(o)*o=PB_EXCEPTCODE_NONE;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionCode(h,o);});}
PbStatus PB_CALL pb_pin_get_faulty_access_address(PbExceptionInfoHandle h,uint64_t*a,uint8_t*k){if(a)*a=0;if(k)*k=0;if(!h||!a||!k)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionFaultAddress(h,a,k);});}
PbStatus PB_CALL pb_pin_get_faulty_access_type(PbExceptionInfoHandle h,PbFaultyAccessType*o){if(o)*o=PB_FAULTY_ACCESS_TYPE_UNKNOWN;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionFaultType(h,o);});}
PbStatus PB_CALL pb_pin_get_fp_error_set(PbExceptionInfoHandle h,uint32_t*o){if(o)*o=0;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionFpErrors(h,o);});}
PbStatus PB_CALL pb_pin_get_windows_exception_argument(PbExceptionInfoHandle h,uint32_t i,uint64_t*o){if(o)*o=0;if(!h||!o||i>=PB_MAX_WINDOWS_EXCEPTION_ARGS)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionWindowsArg(h,i,o);});}
PbStatus PB_CALL pb_pin_get_windows_exception_code(PbExceptionInfoHandle h,uint32_t*o){if(o)*o=0;if(!h||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionWindowsCode(h,o);});}
PbStatus PB_CALL pb_pin_init_access_fault_info(PbExceptionCode c,uint64_t e,uint64_t a,PbFaultyAccessType t,PbExceptionInfoHandle*o){if(o)*o=0;if(!o||!Code(c)||!Type(t))return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionInitAccess(c,e,a,t,o);});}
PbStatus PB_CALL pb_pin_init_exception_info(PbExceptionCode c,uint64_t a,PbExceptionInfoHandle*o){if(o)*o=0;if(!o||!Code(c)||c==PB_EXCEPTCODE_WINDOWS)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionInit(c,a,o);});}
PbStatus PB_CALL pb_pin_init_windows_exception_info(uint32_t c,uint64_t a,const uint64_t*v,uint32_t n,PbExceptionInfoHandle*o){if(o)*o=0;if(!o||n>PB_MAX_WINDOWS_EXCEPTION_ARGS||(!v&&n))return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionInitWindows(c,a,v,n,o);});}
PbStatus PB_CALL pb_pin_raise_exception(PbConstContextHandle c,PbThreadId t,PbExceptionInfoHandle h){if(!c||!h)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionRaise(c,t,h);});}
PbStatus PB_CALL pb_pin_set_exception_address(PbExceptionInfoHandle h,uint64_t a){if(!h)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendExceptionSetAddress(h,a);});}
