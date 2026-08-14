/* Tool-host glue for foreign-language PinTool DLLs (Rust, ...).
 *
 * Pin's Windows tool loader requires the -t DLL to export ClientIntC (and
 * uses PinCommitHashC), which are defined by pin.lib inside this DLL. A
 * consumer DLL written in another language cannot link pin.lib, so it exports
 * thin local stubs that call these two functions instead. These symbols are
 * host-glue only: they are NOT part of the frozen PinBridge C ABI and are not
 * declared in pinbridge.h.
 *
 * The declarations below match the exact symbols in pin.lib (verified against
 * the pinbridge.dll export aliases). */

namespace LEVEL_VM { class PINCLIENTINT; }
namespace LEVEL_PINCLIENT { LEVEL_VM::PINCLIENTINT* __cdecl ClientInt(void); }
namespace LEVEL_BASE { const char* __cdecl PinCommitHash(void); }

extern "C" __declspec(dllexport) void* pb_toolhost_client_int(void)
{
    return LEVEL_PINCLIENT::ClientInt();
}

extern "C" __declspec(dllexport) const char* pb_toolhost_commit_hash(void)
{
    return LEVEL_BASE::PinCommitHash();
}
