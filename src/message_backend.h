#ifndef PINBRIDGE_MESSAGE_BACKEND_H
#define PINBRIDGE_MESSAGE_BACKEND_H
#include "pinbridge/pinbridge.h"
PbStatus PbBackendAssertString(const char*,const char*,uint32_t,const char*,char*,uint64_t,uint64_t*);
PbStatus PbBackendBreakMe(void);
PbStatus PbBackendMillisecondsElapsed(uint64_t*);
#endif
