#include <windows.h>
#include <stdio.h>
#include <string.h>

volatile unsigned char g_pinbridge_rpc_memory[16] = {
    0x10, 0x20, 0x30, 0x40, 0x51, 0x62, 0x73, 0x84,
    0x95, 0xa6, 0xb7, 0xc8, 0xd9, 0xea, 0xfb, 0x0c,
};

volatile unsigned int g_pinbridge_rpc_tick_count;

/* Nonzero asks the main loop to return (clean ExitProcess -> Pin fini
   callbacks run). Tests poke it through the control plane's write op. */
volatile unsigned int g_pinbridge_rpc_exit_flag;

/* Nonzero raises one handled AV per loop iteration; --raise-av sets it at
   startup, tests can also toggle it via the control plane's write op. */
volatile unsigned int g_pinbridge_rpc_raise_av;

/* Nonzero adds a 400-call rpc_tick burst per loop iteration; --spin sets it
   at startup. Gives the trace recording channel a hot main-module window
   while the 100ms pacing (and thus the other e2e timings) stays intact. */
volatile unsigned int g_pinbridge_rpc_spin;

__declspec(dllexport) __declspec(noinline) void rpc_tick(void)
{
    ++g_pinbridge_rpc_tick_count;
}

/* One handled access violation (deref of address 0x1), same pattern as
   fixture.c TriggerHandledException. The agent's context-change callback
   turns it into an exception event for plugins. */
static void TriggerHandledAv(void)
{
    __try
    {
        volatile const unsigned int* invalid_address =
            (volatile const unsigned int*)1u;
        volatile unsigned int value = *invalid_address;
        (void)value;
    }
    __except (EXCEPTION_EXECUTE_HANDLER)
    {
    }
}

int main(int argc, char** argv)
{
    int index;
    for (index = 1; index < argc; ++index)
    {
        if (strcmp(argv[index], "--raise-av") == 0)
            g_pinbridge_rpc_raise_av = 1;
        if (strcmp(argv[index], "--spin") == 0)
            g_pinbridge_rpc_spin = 1;
    }
    for (index = 1; index + 1 < argc; ++index)
    {
        if (strcmp(argv[index], "--pinbridge-rpc-info") == 0)
        {
            FILE* stream = fopen(argv[index + 1], "wb");
            int byte_index;
            if (!stream)
                return 2;
            fprintf(stream, "memory_address=%p\n", (const void*)g_pinbridge_rpc_memory);
            fprintf(stream, "tick_address=%p\n", (const void*)rpc_tick);
            fprintf(stream, "tick_count_address=%p\n", (const void*)&g_pinbridge_rpc_tick_count);
            fprintf(stream, "exit_flag_address=%p\n", (const void*)&g_pinbridge_rpc_exit_flag);
            fprintf(stream, "raise_av_address=%p\n", (const void*)&g_pinbridge_rpc_raise_av);
            fprintf(stream, "memory_hex=");
            for (byte_index = 0; byte_index < 16; ++byte_index)
                fprintf(stream, "%02x", (unsigned int)g_pinbridge_rpc_memory[byte_index]);
            fprintf(stream, "\n");
            fclose(stream);
            break;
        }
    }
    for (;;)
    {
        rpc_tick();
        if (g_pinbridge_rpc_raise_av)
            TriggerHandledAv();
        if (g_pinbridge_rpc_spin)
        {
            unsigned int spin;
            for (spin = 0; spin < 400; ++spin)
                rpc_tick();
        }
        if (g_pinbridge_rpc_exit_flag)
            break;
        Sleep(100);
    }
    return 0;
}
