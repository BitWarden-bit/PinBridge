#ifndef PINBRIDGE_APPDEBUG_BACKEND_H
#define PINBRIDGE_APPDEBUG_BACKEND_H
#include "pinbridge/pinbridge.h"
PbStatus PbBackendImgGetLoaderInfo(PbImgHandle image,uint64_t* out);
PbStatus PbBackendImgSetLoaderInfo(PbImgHandle image,uint64_t value);
PbStatus PbBackendAddBreakpoint(PbDebugBreakpointCallback cb,void* user,uint64_t* out);
PbStatus PbBackendAddInterpreter(PbDebugInterpreterCallback cb,void* user,uint64_t* out);
PbStatus PbBackendAddRegisterEmulator(const PbDebuggerRegDescription* regs,uint32_t count,PbGetEmulatedRegisterCallback get_cb,PbSetEmulatedRegisterCallback set_cb,PbGetTargetDescriptionCallback desc_cb,void* user);
PbStatus PbBackendApplicationBreakpoint(PbConstContextHandle context,PbThreadId tid,uint8_t wait,const char* message);
PbStatus PbBackendChangePending(PbThreadId tid,uint8_t squash,const char* message,uint8_t* changed);
PbStatus PbBackendGetConnection(PbDebugConnectionInfo* info,uint8_t* enabled);
PbStatus PbBackendGetDebugStatus(PbDebugStatus* status);
PbStatus PbBackendGetDebuggerType(PbDebuggerType* type);
PbStatus PbBackendGetPending(PbThreadId tid,char* buffer,uint64_t capacity,uint64_t* required,uint8_t* pending);
PbStatus PbBackendIntercept(PbDebuggingEvent event,PbInterceptDebuggingEventCallback cb,void* user);
PbStatus PbBackendRemoveBreakpoint(PbDebugBreakpointCallback cb);
PbStatus PbBackendRemoveInterpreter(PbDebugInterpreterCallback cb);
PbStatus PbBackendResetBreakpoint(uint64_t address);
PbStatus PbBackendSetDebugMode(const PbDebugMode* mode,uint8_t* accepted);
PbStatus PbBackendWaitDebugger(uint32_t timeout,uint8_t* connected);
#endif
