# oep_watch.py: write-then-execute (OEP candidate) watcher sketch.
#
# Watches memory-write and exec events; an exec from a page that saw
# writes after we started is reported as an OEP candidate. This is the
# scripting-layer counterpart of the old rpc_server oep_trap — cheap to
# iterate on here; anything that proves hot can later move into a native
# engine.
#
# Load:  pinbridge-cli --port 9012 script run oep_watch.py
#
# NOTE: memory/exec events only flow for the trace range the agent was
# launched with (PINBRIDGE_AGENT_RANGE). On real targets narrow the range
# knob first, or the unfiltered flood will bury the ring.

import pb

written = {}        # page base (4k) -> True once written
reported = 0
MAX_REPORTS = 8

def page_of(addr):
    return addr & ~0xFFF

pb.watch(kinds=["memory", "exec"], batch=1024)

def on_event_batch(events, missed):
    global reported
    if missed:
        pb.print("oep_watch: ring overrun, missed=%d" % missed)
    for e in events:
        if e["kind"] == 2 and e["a2"] == 1:
            # memory write: a0 = effective address, a1 = size, a2 = access(1=write)
            written[page_of(e["a0"])] = True
        elif e["kind"] == 3:
            # exec: addr = instruction pointer
            if page_of(e["addr"]) in written and reported < MAX_REPORTS:
                reported += 1
                where = pb.resolve(e["addr"]) or ("0x%x" % e["addr"])
                pb.print("oep_watch: candidate #%d exec of written page: %s (tid=%d)"
                         % (reported, where, e["tid"]))

def on_stop(tid, addr):
    if tid >= 0:
        pb.print("oep_watch: stopped at 0x%x (tid=%d)" % (addr, tid))

pb.print("oep_watch: watching write-then-execute")
