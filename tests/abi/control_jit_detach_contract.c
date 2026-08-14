#include "pinbridge/pinbridge.h"

int main(void)
{
    return pb_pin_detach() == PB_OK ? 0 : 1;
}
