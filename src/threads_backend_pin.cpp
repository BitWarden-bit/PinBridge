#include "pin.H"
#include "threads_backend.h"
PbStatus PbBackendThreadCreateKey(PbTlsDestructor d,PbTlsKey*o){*o=PIN_CreateThreadDataKey(reinterpret_cast<DESTRUCTFUN>(d));return PB_OK;}
PbStatus PbBackendThreadDeleteKey(PbTlsKey k,uint8_t*o){*o=PIN_DeleteThreadDataKey(k)?1:0;return PB_OK;}
PB_NORETURN void PbBackendThreadExit(int32_t c){PIN_ExitThread(c);for(;;){}}
PbStatus PbBackendThreadParentTid(PbOsThreadId*o){*o=PIN_GetParentTid();return PB_OK;}
PbStatus PbBackendStoppedThreadContext(PbThreadId t,PbConstContextHandle*o){*o=reinterpret_cast<PbConstContextHandle>(PIN_GetStoppedThreadContext(t));return PB_OK;}
PbStatus PbBackendStoppedThreadCount(uint32_t*o){*o=PIN_GetStoppedThreadCount();return PB_OK;}
PbStatus PbBackendStoppedThreadId(uint32_t i,PbThreadId*o){*o=PIN_GetStoppedThreadId(i);return PB_OK;}
PbStatus PbBackendStoppedThreadWriteableContext(PbThreadId t,PbContextHandle*o){*o=reinterpret_cast<PbContextHandle>(PIN_GetStoppedThreadWriteableContext(t));return PB_OK;}
PbStatus PbBackendThreadGetData(PbTlsKey k,PbThreadId t,void**o){*o=PIN_GetThreadData(k,t);return PB_OK;}
PbStatus PbBackendThreadTid(PbOsThreadId*o){*o=PIN_GetTid();return PB_OK;}
PbStatus PbBackendThreadIsApplication(uint8_t*o){*o=PIN_IsApplicationThread()?1:0;return PB_OK;}
PbStatus PbBackendThreadIsStoppedInDebugger(PbThreadId t,uint8_t*o){*o=PIN_IsThreadStoppedInDebugger(t)?1:0;return PB_OK;}
PbStatus PbBackendThreadResumeApplication(PbThreadId t){PIN_ResumeApplicationThreads(t);return PB_OK;}
PbStatus PbBackendThreadSetData(PbTlsKey k,const void*d,PbThreadId t,uint8_t*o){*o=PIN_SetThreadData(k,d,t)?1:0;return PB_OK;}
PbStatus PbBackendThreadSleep(uint32_t m){PIN_Sleep(m);return PB_OK;}
PbStatus PbBackendThreadSpawnApplication(PbConstContextHandle c,uint8_t*o){*o=PIN_SpawnApplicationThread(reinterpret_cast<const CONTEXT*>(c))?1:0;return PB_OK;}
PbStatus PbBackendThreadSpawnInternal(PbThreadRootCallback f,void*a,uint64_t s,PbThreadId*t,PbPinThreadUid*u){*t=PIN_SpawnInternalThread(reinterpret_cast<ROOT_THREAD_FUNC*>(f),a,static_cast<size_t>(s),u);return PB_OK;}
PbStatus PbBackendThreadStopApplication(PbThreadId t,uint8_t*o){*o=PIN_StopApplicationThreads(t)?1:0;return PB_OK;}
PbStatus PbBackendThreadId(PbThreadId*o){*o=PIN_ThreadId();return PB_OK;}
PbStatus PbBackendThreadUid(PbPinThreadUid*o){*o=PIN_ThreadUid();return PB_OK;}
PbStatus PbBackendThreadWait(PbPinThreadUid u,uint32_t m,uint8_t*t,int32_t*e){*t=PIN_WaitForThreadTermination(u,m,e)?1:0;return PB_OK;}
PbStatus PbBackendThreadYield(void){PIN_Yield();return PB_OK;}
