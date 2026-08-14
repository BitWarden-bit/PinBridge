#ifndef PINBRIDGE_THREADS_BACKEND_H
#define PINBRIDGE_THREADS_BACKEND_H
#include "pinbridge/pinbridge.h"
PbStatus PbBackendThreadCreateKey(PbTlsDestructor,PbTlsKey*);
PbStatus PbBackendThreadDeleteKey(PbTlsKey,uint8_t*);
PB_NORETURN void PbBackendThreadExit(int32_t);
PbStatus PbBackendThreadParentTid(PbOsThreadId*);
PbStatus PbBackendStoppedThreadContext(PbThreadId,PbConstContextHandle*);
PbStatus PbBackendStoppedThreadCount(uint32_t*);
PbStatus PbBackendStoppedThreadId(uint32_t,PbThreadId*);
PbStatus PbBackendStoppedThreadWriteableContext(PbThreadId,PbContextHandle*);
PbStatus PbBackendThreadGetData(PbTlsKey,PbThreadId,void**);
PbStatus PbBackendThreadTid(PbOsThreadId*);
PbStatus PbBackendThreadIsApplication(uint8_t*);
PbStatus PbBackendThreadIsStoppedInDebugger(PbThreadId,uint8_t*);
PbStatus PbBackendThreadResumeApplication(PbThreadId);
PbStatus PbBackendThreadSetData(PbTlsKey,const void*,PbThreadId,uint8_t*);
PbStatus PbBackendThreadSleep(uint32_t);
PbStatus PbBackendThreadSpawnApplication(PbConstContextHandle,uint8_t*);
PbStatus PbBackendThreadSpawnInternal(PbThreadRootCallback,void*,uint64_t,PbThreadId*,PbPinThreadUid*);
PbStatus PbBackendThreadStopApplication(PbThreadId,uint8_t*);
PbStatus PbBackendThreadId(PbThreadId*);
PbStatus PbBackendThreadUid(PbPinThreadUid*);
PbStatus PbBackendThreadWait(PbPinThreadUid,uint32_t,uint8_t*,int32_t*);
PbStatus PbBackendThreadYield(void);
#endif
