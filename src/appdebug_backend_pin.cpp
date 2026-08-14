#include "pin.H"
#include "appdebug_backend.h"
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
namespace {
struct BreakState{PbDebugBreakpointCallback cb;void*user;}; struct InterpState{PbDebugInterpreterCallback cb;void*user;};
BreakState*g_break; InterpState*g_interp;
PbGetEmulatedRegisterCallback g_get; PbSetEmulatedRegisterCallback g_set; PbGetTargetDescriptionCallback g_desc; void*g_emu_user;
PbInterceptDebuggingEventCallback g_events[3]; void*g_event_users[3];
std::vector<std::string> g_reg_names; std::vector<DEBUGGER_REG_DESCRIPTION> g_reg_desc;
IMG Img(PbImgHandle h){IMG i;i.q_set(h.opaque);return i;}
BOOL BreakThunk(ADDRINT a,UINT s,BOOL i,VOID*v){BreakState*x=static_cast<BreakState*>(v);return x->cb(a,s,i?1:0,x->user)!=0;}
BOOL InterpThunk(THREADID t,CONTEXT*c,const std::string&cmd,std::string*reply,VOID*v){InterpState*x=static_cast<InterpState*>(v);const char*out=0;uint8_t handled=x->cb(t,reinterpret_cast<PbContextHandle>(c),cmd.c_str(),&out,x->user);if(handled&&out)*reply=out;return handled!=0;}
VOID GetThunk(unsigned id,THREADID t,CONTEXT*c,VOID*d,VOID*){g_get(id,t,reinterpret_cast<PbContextHandle>(c),d,g_emu_user);}
VOID SetThunk(unsigned id,THREADID t,CONTEXT*c,const VOID*d,VOID*){g_set(id,t,reinterpret_cast<PbContextHandle>(c),d,g_emu_user);}
USIZE DescThunk(const std::string&n,USIZE s,VOID*b,VOID*){return static_cast<USIZE>(g_desc(n.c_str(),s,b,g_emu_user));}
BOOL EventThunk0(THREADID t,DEBUGGING_EVENT e,CONTEXT*c,VOID*){return g_events[0](t,e,reinterpret_cast<PbContextHandle>(c),g_event_users[0])!=0;}
BOOL EventThunk1(THREADID t,DEBUGGING_EVENT e,CONTEXT*c,VOID*){return g_events[1](t,e,reinterpret_cast<PbContextHandle>(c),g_event_users[1])!=0;}
BOOL EventThunk2(THREADID t,DEBUGGING_EVENT e,CONTEXT*c,VOID*){return g_events[2](t,e,reinterpret_cast<PbContextHandle>(c),g_event_users[2])!=0;}
INTERCEPT_DEBUGGING_EVENT_CALLBACK EventThunk(unsigned e){return e==0?EventThunk0:e==1?EventThunk1:EventThunk2;}
PbStatus Jit(){return PIN_IsProbeMode()?PB_ERR_INVALID_STATE:PB_OK;}
}
static_assert(PB_DEBUGGER_TYPE_VISUAL_STUDIO==DEBUGGER_TYPE_VISUAL_STUDIO,"debugger type drift"); static_assert(PB_DEBUGGING_EVENT_ASYNC_BREAK==DEBUGGING_EVENT_ASYNC_BREAK,"debug event drift"); static_assert(PB_DEBUG_CONNECTION_TYPE_TCP_CLIENT==DEBUG_CONNECTION_TYPE_TCP_CLIENT,"debug connection drift"); static_assert(PB_DEBUG_MODE_OPTION_ALLOW_REMOTE==DEBUG_MODE_OPTION_ALLOW_REMOTE,"debug option drift"); static_assert(PB_DEBUG_STATUS_CONNECTED==DEBUG_STATUS_CONNECTED,"debug status drift");
PbStatus PbBackendImgGetLoaderInfo(PbImgHandle i,uint64_t*o){*o=reinterpret_cast<uint64_t>(IMG_GetLoaderInfo(Img(i)));return PB_OK;}
PbStatus PbBackendImgSetLoaderInfo(PbImgHandle i,uint64_t v){IMG_SetLoaderInfo(Img(i),reinterpret_cast<void*>(static_cast<uintptr_t>(v)));return PB_OK;}
PbStatus PbBackendAddBreakpoint(PbDebugBreakpointCallback c,void*u,uint64_t*o){if(Jit()!=PB_OK)return Jit();if(g_break)return PB_ERR_INVALID_STATE;g_break=static_cast<BreakState*>(std::malloc(sizeof(BreakState)));if(!g_break)return PB_ERR_OUT_OF_MEMORY;g_break->cb=c;g_break->user=u;PIN_CALLBACK h=PIN_AddBreakpointHandler(BreakThunk,g_break);if(h==PIN_CALLBACK_INVALID){std::free(g_break);g_break=0;return PB_ERR_PIN_REJECTED_ARGUMENTS;}*o=reinterpret_cast<uint64_t>(h);return PB_OK;}
PbStatus PbBackendAddInterpreter(PbDebugInterpreterCallback c,void*u,uint64_t*o){if(Jit()!=PB_OK)return Jit();if(g_interp)return PB_ERR_INVALID_STATE;g_interp=static_cast<InterpState*>(std::malloc(sizeof(InterpState)));if(!g_interp)return PB_ERR_OUT_OF_MEMORY;g_interp->cb=c;g_interp->user=u;PIN_CALLBACK h=PIN_AddDebugInterpreter(InterpThunk,g_interp);if(h==PIN_CALLBACK_INVALID){std::free(g_interp);g_interp=0;return PB_ERR_PIN_REJECTED_ARGUMENTS;}*o=reinterpret_cast<uint64_t>(h);return PB_OK;}
PbStatus PbBackendAddRegisterEmulator(const PbDebuggerRegDescription*r,uint32_t n,PbGetEmulatedRegisterCallback g,PbSetEmulatedRegisterCallback s,PbGetTargetDescriptionCallback d,void*u){if(Jit()!=PB_OK)return Jit();g_reg_names.clear();g_reg_desc.clear();g_reg_names.reserve(n);g_reg_desc.reserve(n);for(uint32_t i=0;i<n;i++)g_reg_names.push_back(r[i].name);for(uint32_t i=0;i<n;i++){DEBUGGER_REG_DESCRIPTION x={static_cast<REG>(r[i].pin_reg),r[i].tool_reg_id,r[i].width_in_bits,g_reg_names[i].c_str(),r[i].gcc_id};g_reg_desc.push_back(x);}g_get=g;g_set=s;g_desc=d;g_emu_user=u;PIN_AddDebuggerRegisterEmulator(n,&g_reg_desc[0],GetThunk,SetThunk,DescThunk,0);return PB_OK;}
PbStatus PbBackendApplicationBreakpoint(PbConstContextHandle c,PbThreadId t,uint8_t w,const char*m){if(Jit()!=PB_OK)return Jit();PIN_ApplicationBreakpoint(reinterpret_cast<const CONTEXT*>(c),t,w!=0,m);return PB_ERR_INTERNAL;}
PbStatus PbBackendChangePending(PbThreadId t,uint8_t s,const char*m,uint8_t*o){if(Jit()!=PB_OK)return Jit();*o=PIN_ChangePendingToolBreakpointOnStoppedThread(t,s!=0,m)?1:0;return PB_OK;}
PbStatus PbBackendGetConnection(PbDebugConnectionInfo*i,uint8_t*e){if(Jit()!=PB_OK)return Jit();DEBUG_CONNECTION_INFO x={};*e=PIN_GetDebugConnectionInfo(&x)?1:0;i->type=x._type;i->stop_at_entry=x._stopAtEntry?1:0;i->tcp_port=x._tcpServer._tcpPort;return PB_OK;}
PbStatus PbBackendGetDebugStatus(PbDebugStatus*o){if(Jit()!=PB_OK)return Jit();*o=PIN_GetDebugStatus();return PB_OK;} PbStatus PbBackendGetDebuggerType(PbDebuggerType*o){if(Jit()!=PB_OK)return Jit();*o=PIN_GetDebuggerType();return PB_OK;}
PbStatus PbBackendGetPending(PbThreadId t,char*b,uint64_t c,uint64_t*r,uint8_t*p){if(Jit()!=PB_OK)return Jit();std::string s;*p=PIN_GetStoppedThreadPendingToolBreakpoint(t,&s)?1:0;*r=s.size()+1;if(!b||c<*r)return PB_ERR_BUFFER_TOO_SMALL;std::memcpy(b,s.c_str(),static_cast<size_t>(*r));return PB_OK;}
PbStatus PbBackendIntercept(PbDebuggingEvent e,PbInterceptDebuggingEventCallback c,void*u){if(Jit()!=PB_OK)return Jit();g_events[e]=c;g_event_users[e]=u;PIN_InterceptDebuggingEvent(static_cast<DEBUGGING_EVENT>(e),c?EventThunk(e):0,0);return PB_OK;}
PbStatus PbBackendRemoveBreakpoint(PbDebugBreakpointCallback c){if(Jit()!=PB_OK)return Jit();if(!g_break||g_break->cb!=c)return PB_ERR_INVALID_STATE;PIN_RemoveBreakpointHandler(BreakThunk);std::free(g_break);g_break=0;return PB_OK;}
PbStatus PbBackendRemoveInterpreter(PbDebugInterpreterCallback c){if(Jit()!=PB_OK)return Jit();if(!g_interp||g_interp->cb!=c)return PB_ERR_INVALID_STATE;PIN_RemoveDebugInterpreter(InterpThunk);std::free(g_interp);g_interp=0;return PB_OK;}
PbStatus PbBackendResetBreakpoint(uint64_t a){if(Jit()!=PB_OK)return Jit();PIN_ResetBreakpointAt(a);return PB_OK;}
PbStatus PbBackendSetDebugMode(const PbDebugMode*m,uint8_t*o){if(Jit()!=PB_OK)return Jit();DEBUG_MODE x={};x._type=static_cast<DEBUG_CONNECTION_TYPE>(m->type);x._options=m->options;x._tcpClient._ip=m->tcp_client_ip;x._tcpClient._tcpPort=m->tcp_port;*o=PIN_SetDebugMode(&x)?1:0;return PB_OK;}
PbStatus PbBackendWaitDebugger(uint32_t t,uint8_t*o){if(Jit()!=PB_OK)return Jit();*o=PIN_WaitForDebuggerToConnect(t)?1:0;return PB_OK;}
