# ntdll_trace.py: hook every ntdll export and watch what the app calls.
#
# Resolves all exports of ntdll.dll, plants one hook point per unique export
# address (capped at MAX_HOOKS), then prints the first DETAIL_PER_NAME hits with
# its Win64 argument registers, plus a totals line every SUMMARY_EVERY hits.
#
# Load:  pinbridge-cli --port 9012 script run ntdll_trace.py
# Read:  pinbridge-cli --port 9012 script output --follow
#
# NOTE: runtime hook points are range-independent — they fire no matter
# what trace range the agent was launched with. engines.rs on_ins checks
# the runtime hook set separately from PINBRIDGE_AGENT_RANGE, and hook_set
# flushes the JIT cache for the address, so hooks armed at runtime catch
# already-compiled code the next time it executes.

import pb

MODULE = "ntdll.dll"
MAX_HOOKS = 4096        # agent-side hook point cap
DETAIL_PER_NAME = 3     # full lines per function before going quiet
SUMMARY_EVERY = 1000    # totals line cadence

names = {}              # hooked addr -> export name (per-hit dict lookup is
                        # MUCH cheaper than pb.resolve on every event)
armed = 0
exports_seen = 0
total = 0
per_name = {}           # name -> hit count

def pb_init():
    global armed, exports_seen
    for addr, name in pb.exports(MODULE):
        exports_seen += 1
        if addr in names:
            continue  # Nt*/Zw* and other aliases commonly share an entry
        if armed >= MAX_HOOKS:
            break
        if pb.hook_set(addr):
            names[addr] = name
            armed += 1
    pb.print("ntdll_trace: %d unique hooks armed from %d exports on %s"
             % (armed, exports_seen, MODULE))

pb.watch(kinds=["hook"], batch=1024)

def on_event_batch(events, missed):
    global total
    if missed:
        pb.print("ntdll_trace: ring overrun, missed=%d" % missed)
    for e in events:
        if e["kind"] != 1:
            continue  # hook_regs only: a0=rcx a1=rdx a2=r8 a3=r9
        name = names.get(e["addr"])
        if name is None:
            continue  # hook from another plugin/script
        total += 1
        seen = per_name.get(name, 0) + 1
        per_name[name] = seen
        if seen <= DETAIL_PER_NAME:
            pb.print("NTDLL_HIT %s(rcx=0x%x rdx=0x%x r8=0x%x r9=0x%x) tid=%d"
                     % (name, e["a0"], e["a1"], e["a2"], e["a3"], e["tid"]))
        if total % SUMMARY_EVERY == 0:
            top = sorted(per_name.items(), key=lambda kv: -kv[1])[:8]
            pb.print("ntdll_trace: %d hits, %d distinct; top: %s"
                     % (total, len(per_name),
                        ", ".join("%s=%d" % kv for kv in top)))

def on_unload():
    pb.hook_clear()
    pb.print("ntdll_trace: unloaded, hooks cleared (%d hits total)" % total)
