#include <stdint.h>
#include <string.h>
#include "pinbridge/pinbridge.h"
int main(void){PbKnobHandle a=0,b=0;uint32_t count=0;uint8_t value=0;int32_t compare=0;char text[64];uint64_t required=0;
 if(sizeof(PbKnobMode)!=4||PB_KNOB_MODE_LAST!=6)return 1;
 if(pb_knob_check_all(0)!=PB_OK||pb_knob_count(&count)!=PB_OK||count!=2||pb_knob_slow_asserts(&value)!=PB_OK)return 2;
 if(pb_knob_find("alpha",&a)!=PB_OK||!a||pb_knob_find_family("bridge",&b)!=PB_OK||!b||pb_knob_compare(a,a,&compare)!=PB_OK||compare!=0)return 3;
 if(pb_knob_set_by_user(a,&value)!=PB_OK||value||pb_knob_turn_on_set_by_user(a)!=PB_OK||pb_knob_set_by_user(a,&value)!=PB_OK||!value)return 6;
 if(pb_knob_summary(0,0,&required)!=PB_ERR_BUFFER_TOO_SMALL||required>sizeof(text)||pb_knob_summary(text,sizeof(text),&required)!=PB_OK||strstr(text,"alpha")==0||pb_knob_long_all(text,sizeof(text),&required)!=PB_OK)return 7;
 if(pb_knob_check_all(2)!=PB_ERR_INVALID_ARGUMENT||pb_knob_find(0,&a)!=PB_ERR_INVALID_ARGUMENT||pb_knob_compare(0,b,&compare)!=PB_ERR_INVALID_ARGUMENT||pb_knob_count(0)!=PB_ERR_INVALID_ARGUMENT)return 8;return 0;}
