/*
 * hello32.c — minimal, self-contained 32-bit PE fixture for pinbridge tests.
 *
 * The program performs two recognizable operations — a byte reversal and a
 * running rotate-and-XOR checksum over a fixed buffer — and one small branch
 * function, then prints its own result and exits. It reads no files, takes no
 * arguments, and never touches a debugger or any external sample: its only
 * side effect is the single line written to stdout.
 *
 * Built as a PE32 (I386) image by fixtures/x86/build.ps1; see
 * fixtures/x86/README.md for what to expect in the headers.
 */

#include <stdio.h>
#include <string.h>

/* Fixed input buffer the whole program operates on. */
static const char INPUT[] = "pinbridge-ia32";

/* Fills dst with the byte-reversed contents of src (dst must be len+1). */
static void reverse_bytes(const char *src, char *dst, size_t len)
{
    size_t i;
    for (i = 0; i < len; ++i) {
        dst[i] = src[len - 1 - i];
    }
    dst[len] = '\0';
}

/* Running rotate-left-then-XOR checksum over a buffer: a tight, recognizable
 * loop with no memory loads beyond the buffer itself. */
static unsigned int checksum(const char *buf, size_t len)
{
    unsigned int acc = 0;
    size_t i;
    for (i = 0; i < len; ++i) {
        acc ^= (unsigned char)buf[i];
        acc = (acc << 1) | (acc >> ((sizeof(acc) * 8u) - 1u));
    }
    return acc;
}

/* Small decision function that exercises a recognizable branch ladder. */
static unsigned int classify(unsigned int value)
{
    if (value == 0) {
        return 0;
    } else if (value < 0x100u) {
        return 1;
    } else if ((value & 1u) == 0) {
        return 2;
    }
    return 3;
}

int main(void)
{
    char reversed[sizeof(INPUT)];
    unsigned int sum;

    reverse_bytes(INPUT, reversed, strlen(INPUT));
    sum = checksum(reversed, strlen(reversed));

    printf("hello32: input=%s reversed=%s checksum=%08x class=%u\n",
           INPUT, reversed, sum, classify(sum));
    return 0;
}
