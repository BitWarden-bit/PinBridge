#include <stdint.h>
#include <string.h>
#include "pinbridge/pinbridge.h"
int main(void){char b[256]={};uint64_t r=0,t=1;if(PB_LOGTYPE_CONSOLE!=0||PB_LOGTYPE_CONSOLE_AND_LOGFILE!=2||PB_MESSAGE_KIND_LOG!=8)return 1;if(pb_assert_string("a.cpp","f",7,"bad",0,0,&r)!=PB_ERR_BUFFER_TOO_SMALL||r>sizeof(b)||pb_assert_string("a.cpp","f",7,"bad",b,sizeof(b),&r)!=PB_OK||strstr(b,"bad")==0)return 2;if(pb_milliseconds_elapsed(&t)!=PB_OK)return 3;if(pb_assert_string(0,"f",0,"x",b,sizeof(b),&r)!=PB_ERR_INVALID_ARGUMENT||pb_milliseconds_elapsed(0)!=PB_ERR_INVALID_ARGUMENT)return 4;return 0;}
