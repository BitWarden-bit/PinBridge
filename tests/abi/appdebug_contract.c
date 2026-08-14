#include <stdint.h>
#include <string.h>
#include "pinbridge/pinbridge.h"
static uint8_t PB_CALL on_break(uint64_t a,uint32_t s,uint8_t i,void*u){(void)a;(void)s;(void)i;(void)u;return 1;}
static uint8_t PB_CALL on_command(PbThreadId t,PbContextHandle c,const char*cmd,const char**reply,void*u){(void)t;(void)c;(void)cmd;(void)u;*reply="ok";return 1;}
static void PB_CALL on_get(uint32_t r,PbThreadId t,PbContextHandle c,void*d,void*u){(void)r;(void)t;(void)c;(void)d;(void)u;}
static void PB_CALL on_set(uint32_t r,PbThreadId t,PbContextHandle c,const void*d,void*u){(void)r;(void)t;(void)c;(void)d;(void)u;}
static uint64_t PB_CALL on_desc(const char*n,uint64_t s,void*b,void*u){(void)n;(void)s;(void)b;(void)u;return 0;}
static uint8_t PB_CALL on_event(PbThreadId t,PbDebuggingEvent e,PbContextHandle c,void*u){(void)t;(void)e;(void)c;(void)u;return 1;}
int main(void){PbImgHandle image={7};uint64_t value=0,required=0;PbCallbackHandle h={0};PbDebugStatus status=99;PbDebuggerType type=99;PbDebugConnectionInfo info;uint8_t flag=9;char text[4];
 PbDebuggerRegDescription reg={0,1,64,"mockreg",2};PbDebugMode mode={PB_DEBUG_CONNECTION_TYPE_NONE,PB_DEBUG_MODE_OPTION_NONE,0,0,0};
 if(sizeof(PbDebuggerType)!=4||PB_DEBUGGER_TYPE_VISUAL_STUDIO!=5||PB_DEBUG_MODE_OPTION_ALLOW_REMOTE!=4||PB_DEBUG_STATUS_CONNECTED!=3)return 1;
 if(pb_img_set_loader_info(image,UINT64_C(0x1234))!=PB_OK||pb_img_get_loader_info(image,&value)!=PB_OK||value!=UINT64_C(0x1234))return 2;
 if(pb_pin_add_breakpoint_handler(on_break,0,&h)!=PB_OK||h.opaque!=31||pb_pin_remove_breakpoint_handler(on_break)!=PB_OK)return 3;
 if(pb_pin_add_debug_interpreter(on_command,0,&h)!=PB_OK||h.opaque!=32||pb_pin_remove_debug_interpreter(on_command)!=PB_OK)return 4;
 if(pb_pin_add_debugger_register_emulator(&reg,1,on_get,on_set,on_desc,0)!=PB_OK||pb_pin_intercept_debugging_event(PB_DEBUGGING_EVENT_BREAKPOINT,on_event,0)!=PB_OK)return 5;
 if(pb_pin_get_debug_status(&status)!=PB_OK||status!=PB_DEBUG_STATUS_DISABLED||pb_pin_get_debugger_type(&type)!=PB_OK||type!=PB_DEBUGGER_TYPE_UNKNOWN)return 6;
 if(pb_pin_get_debug_connection_info(&info,&flag)!=PB_OK||flag||info.type!=PB_DEBUG_CONNECTION_TYPE_NONE)return 7;
 if(pb_pin_get_pending_tool_breakpoint(0,0,0,&required,&flag)!=PB_ERR_BUFFER_TOO_SMALL||required!=1||flag||pb_pin_get_pending_tool_breakpoint(0,text,sizeof(text),&required,&flag)!=PB_OK||text[0])return 8;
 if(pb_pin_set_debug_mode(&mode,&flag)!=PB_OK||!flag||pb_pin_wait_for_debugger(1,&flag)!=PB_OK||flag||pb_pin_change_pending_tool_breakpoint(0,0,"x",&flag)!=PB_OK||flag||pb_pin_reset_breakpoint_at(0x10)!=PB_OK)return 9;
 if(pb_pin_intercept_debugging_event(99,on_event,0)!=PB_ERR_INVALID_ARGUMENT||pb_pin_set_debug_mode(0,&flag)!=PB_ERR_INVALID_ARGUMENT||pb_pin_get_debug_status(0)!=PB_ERR_INVALID_ARGUMENT||pb_pin_application_breakpoint(0,0,0,"x")!=PB_ERR_INVALID_ARGUMENT)return 10;
 return 0;}
