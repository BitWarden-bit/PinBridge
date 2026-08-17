#include <stdint.h>
#include <string.h>
#include "pinbridge/pinbridge.h"
int main(void){PbExceptionInfoHandle h=0;PbExceptionCode code=0;PbExceptionClass cls=0;PbFaultyAccessType type=0;uint64_t v=0,args[2]={11,22},required=0;uint32_t n=0;uint8_t known=0;char text[32];
 if(PB_EXCEPTCODE_RECEIVED_AMBIGUOUS_SIMD!=31||PB_EXCEPTCLASS_OS!=8||PB_FPERROR_X87_STACK_ERROR!=64||PB_MAX_WINDOWS_EXCEPTION_ARGS!=5)return 1;
 if(pb_pin_init_access_fault_info(PB_EXCEPTCODE_ACCESS_DENIED,0x100,0x200,PB_FAULTY_ACCESS_WRITE,&h)!=PB_OK||!h)return 2;
 if(pb_pin_get_exception_code(h,&code)!=PB_OK||code!=PB_EXCEPTCODE_ACCESS_DENIED||pb_pin_get_exception_class(code,&cls)!=PB_OK||cls!=PB_EXCEPTCLASS_ACCESS_FAULT)return 3;
 if(pb_pin_get_exception_address(h,&v)!=PB_OK||v!=0x100||pb_pin_get_faulty_access_address(h,&v,&known)!=PB_OK||!known||v!=0x200||pb_pin_get_faulty_access_type(h,&type)!=PB_OK||type!=PB_FAULTY_ACCESS_WRITE)return 4;
 if(pb_pin_set_exception_address(h,0x300)!=PB_OK||pb_pin_get_exception_address(h,&v)!=PB_OK||v!=0x300||pb_pin_exception_to_string(h,0,0,&required)!=PB_ERR_BUFFER_TOO_SMALL||pb_pin_exception_to_string(h,text,sizeof(text),&required)!=PB_OK||strstr(text,"mock")==0)return 5;
 if(pb_exception_info_release(h)!=PB_OK)return 6;
 if(pb_pin_init_windows_exception_info(0xdead,0x400,args,2,&h)!=PB_OK||pb_pin_count_windows_exception_arguments(h,&n)!=PB_OK||n!=2||pb_pin_get_windows_exception_code(h,&n)!=PB_OK||n!=0xdead||pb_pin_get_windows_exception_argument(h,1,&v)!=PB_OK||v!=22)return 7;
 v=1;known=1;type=PB_FAULTY_ACCESS_WRITE;
 if(pb_pin_get_faulty_access_address(h,&v,&known)!=PB_ERR_INVALID_ARGUMENT||v!=0||known!=0||pb_pin_get_faulty_access_type(h,&type)!=PB_ERR_INVALID_ARGUMENT||type!=PB_FAULTY_ACCESS_TYPE_UNKNOWN)return 8;
 if(pb_pin_get_windows_exception_argument(h,2,&v)!=PB_ERR_INVALID_ARGUMENT||pb_exception_info_release(h)!=PB_OK)return 9;
 if(pb_pin_init_exception_info(PB_EXCEPTCODE_WINDOWS,0,&h)!=PB_ERR_INVALID_ARGUMENT||pb_pin_init_windows_exception_info(0,0,args,6,&h)!=PB_ERR_INVALID_ARGUMENT||pb_pin_get_exception_code(0,&code)!=PB_ERR_INVALID_ARGUMENT)return 10;
 return 0;}
