#include "pinbridge/pinbridge.h"
#include "threads_backend.h"
namespace{template<class F>PbStatus G(F f){
#if defined(_CPPUNWIND)||defined(__cpp_exceptions)
try{return f();}catch(...){return PB_ERR_INTERNAL;}
#else
return f();
#endif
}}
PbStatus PB_CALL pb_pin_create_thread_data_key(PbTlsDestructor d,PbTlsKey*o){if(o)*o=PB_INVALID_TLS_KEY;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadCreateKey(d,o);});}
PbStatus PB_CALL pb_pin_delete_thread_data_key(PbTlsKey k,uint8_t*o){if(o)*o=0;if(!o||k==PB_INVALID_TLS_KEY)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadDeleteKey(k,o);});}
PB_NORETURN void PB_CALL pb_pin_exit_thread(int32_t c){PbBackendThreadExit(c);}
PbStatus PB_CALL pb_pin_get_parent_tid(PbOsThreadId*o){if(o)*o=PB_INVALID_OS_THREAD_ID;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadParentTid(o);});}
PbStatus PB_CALL pb_pin_get_stopped_thread_context(PbThreadId t,PbConstContextHandle*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendStoppedThreadContext(t,o);});}
PbStatus PB_CALL pb_pin_get_stopped_thread_count(uint32_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendStoppedThreadCount(o);});}
PbStatus PB_CALL pb_pin_get_stopped_thread_id(uint32_t i,PbThreadId*o){if(o)*o=PB_INVALID_THREAD_ID;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendStoppedThreadId(i,o);});}
PbStatus PB_CALL pb_pin_get_stopped_thread_writeable_context(PbThreadId t,PbContextHandle*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendStoppedThreadWriteableContext(t,o);});}
PbStatus PB_CALL pb_pin_get_thread_data(PbTlsKey k,PbThreadId t,void**o){if(o)*o=0;if(!o||k==PB_INVALID_TLS_KEY)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadGetData(k,t,o);});}
PbStatus PB_CALL pb_pin_get_tid(PbOsThreadId*o){if(o)*o=PB_INVALID_OS_THREAD_ID;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadTid(o);});}
PbStatus PB_CALL pb_pin_is_application_thread(uint8_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadIsApplication(o);});}
PbStatus PB_CALL pb_pin_is_thread_stopped_in_debugger(PbThreadId t,uint8_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadIsStoppedInDebugger(t,o);});}
PbStatus PB_CALL pb_pin_resume_application_threads(PbThreadId t){return G([&](){return PbBackendThreadResumeApplication(t);});}
PbStatus PB_CALL pb_pin_set_thread_data(PbTlsKey k,const void*d,PbThreadId t,uint8_t*o){if(o)*o=0;if(!o||k==PB_INVALID_TLS_KEY)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadSetData(k,d,t,o);});}
PbStatus PB_CALL pb_pin_sleep(uint32_t m){return G([&](){return PbBackendThreadSleep(m);});}
PbStatus PB_CALL pb_pin_spawn_application_thread(PbConstContextHandle c,uint8_t*o){if(o)*o=0;if(!c||!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadSpawnApplication(c,o);});}
PbStatus PB_CALL pb_pin_spawn_internal_thread(PbThreadRootCallback f,void*a,uint64_t s,PbThreadId*t,PbPinThreadUid*u){if(t)*t=PB_INVALID_THREAD_ID;if(u)*u=PB_INVALID_PIN_THREAD_UID;if(!f||!t||!u)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadSpawnInternal(f,a,s,t,u);});}
PbStatus PB_CALL pb_pin_stop_application_threads(PbThreadId t,uint8_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadStopApplication(t,o);});}
PbStatus PB_CALL pb_pin_thread_id(PbThreadId*o){if(o)*o=PB_INVALID_THREAD_ID;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadId(o);});}
PbStatus PB_CALL pb_pin_thread_uid(PbPinThreadUid*o){if(o)*o=PB_INVALID_PIN_THREAD_UID;if(!o)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadUid(o);});}
PbStatus PB_CALL pb_pin_wait_for_thread_termination(PbPinThreadUid u,uint32_t m,uint8_t*t,int32_t*e){if(t)*t=0;if(e)*e=0;if(!t||!e||u==PB_INVALID_PIN_THREAD_UID)return PB_ERR_INVALID_ARGUMENT;return G([&](){return PbBackendThreadWait(u,m,t,e);});}
PbStatus PB_CALL pb_pin_yield(void){return G([](){return PbBackendThreadYield();});}
