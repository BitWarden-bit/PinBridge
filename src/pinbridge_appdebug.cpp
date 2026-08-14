#include "pinbridge/pinbridge.h"
#include "appdebug_backend.h"
namespace { template<class F> PbStatus Guard(F f){
#if defined(_CPPUNWIND) || defined(__cpp_exceptions)
try{return f();}catch(...){return PB_ERR_INTERNAL;}
#else
return f();
#endif
} bool Event(PbDebuggingEvent e){return e<=PB_DEBUGGING_EVENT_ASYNC_BREAK;} }
PbStatus PB_CALL pb_img_get_loader_info(PbImgHandle i,uint64_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendImgGetLoaderInfo(i,o);});}
PbStatus PB_CALL pb_img_set_loader_info(PbImgHandle i,uint64_t v){return Guard([&](){return PbBackendImgSetLoaderInfo(i,v);});}
PbStatus PB_CALL pb_pin_add_breakpoint_handler(PbDebugBreakpointCallback c,void*u,PbCallbackHandle*o){if(o)o->opaque=0;if(!c||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendAddBreakpoint(c,u,&o->opaque);});}
PbStatus PB_CALL pb_pin_add_debug_interpreter(PbDebugInterpreterCallback c,void*u,PbCallbackHandle*o){if(o)o->opaque=0;if(!c||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendAddInterpreter(c,u,&o->opaque);});}
PbStatus PB_CALL pb_pin_add_debugger_register_emulator(const PbDebuggerRegDescription*r,uint32_t n,PbGetEmulatedRegisterCallback g,PbSetEmulatedRegisterCallback s,PbGetTargetDescriptionCallback d,void*u){if(!r||!n||!g||!s||!d)return PB_ERR_INVALID_ARGUMENT;for(uint32_t i=0;i<n;i++)if(!r[i].name||!r[i].width_in_bits||(r[i].width_in_bits%8))return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendAddRegisterEmulator(r,n,g,s,d,u);});}
PbStatus PB_CALL pb_pin_application_breakpoint(PbConstContextHandle c,PbThreadId t,uint8_t w,const char*m){if(!c||w>1||!m)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendApplicationBreakpoint(c,t,w,m);});}
PbStatus PB_CALL pb_pin_change_pending_tool_breakpoint(PbThreadId t,uint8_t s,const char*m,uint8_t*o){if(o)*o=0;if(s>1||!m||!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendChangePending(t,s,m,o);});}
PbStatus PB_CALL pb_pin_get_debug_connection_info(PbDebugConnectionInfo*i,uint8_t*e){if(i)*i=PbDebugConnectionInfo{};if(e)*e=0;if(!i||!e)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendGetConnection(i,e);});}
PbStatus PB_CALL pb_pin_get_debug_status(PbDebugStatus*o){if(o)*o=PB_DEBUG_STATUS_DISABLED;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendGetDebugStatus(o);});}
PbStatus PB_CALL pb_pin_get_debugger_type(PbDebuggerType*o){if(o)*o=PB_DEBUGGER_TYPE_UNKNOWN;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendGetDebuggerType(o);});}
PbStatus PB_CALL pb_pin_get_pending_tool_breakpoint(PbThreadId t,char*b,uint64_t c,uint64_t*r,uint8_t*p){if(r)*r=0;if(p)*p=0;if(!r||!p||(!b&&c))return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendGetPending(t,b,c,r,p);});}
PbStatus PB_CALL pb_pin_intercept_debugging_event(PbDebuggingEvent e,PbInterceptDebuggingEventCallback c,void*u){if(!Event(e))return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendIntercept(e,c,u);});}
PbStatus PB_CALL pb_pin_remove_breakpoint_handler(PbDebugBreakpointCallback c){if(!c)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendRemoveBreakpoint(c);});}
PbStatus PB_CALL pb_pin_remove_debug_interpreter(PbDebugInterpreterCallback c){if(!c)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendRemoveInterpreter(c);});}
PbStatus PB_CALL pb_pin_reset_breakpoint_at(uint64_t a){return Guard([&](){return PbBackendResetBreakpoint(a);});}
PbStatus PB_CALL pb_pin_set_debug_mode(const PbDebugMode*m,uint8_t*o){if(o)*o=0;if(!m||!o||m->type>PB_DEBUG_CONNECTION_TYPE_TCP_CLIENT||(m->options&~7u))return PB_ERR_INVALID_ARGUMENT;if(m->type==PB_DEBUG_CONNECTION_TYPE_TCP_CLIENT&&!m->tcp_client_ip)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendSetDebugMode(m,o);});}
PbStatus PB_CALL pb_pin_wait_for_debugger(uint32_t t,uint8_t*o){if(o)*o=0;if(!o)return PB_ERR_INVALID_ARGUMENT;return Guard([&](){return PbBackendWaitDebugger(t,o);});}
