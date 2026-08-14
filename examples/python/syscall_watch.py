# syscall_watch.py: observe native syscalls (Nt* syscalls) executed by the app.
#
# Default mode subscribes to ALL syscall numbers and prints full detail for
# the first DETAIL_FIRST events, then a per-number count table every
# SUMMARY_EVERY events — run it once against your sample to discover which
# numbers it uses, then edit NUMBERS below to focus on those.
#
# Load:  pinbridge-cli --port 9012 script run syscall_watch.py
# Read:  pinbridge-cli --port 9012 script output --follow
#
# Syscall numbers vary per Windows build — do NOT hardcode from a table
# found online; discover them with this script on the actual target OS.
# Common ones on recent Win10/11 x64 (verify before relying on them):
#   0x03 NtReadFile   0x05 NtWriteFile   0x0f NtOpenKey
#   0x18 NtAllocateVirtualMemory         0x3a NtWriteVirtualMemory
#   0x50 NtProtectVirtualMemory          0x55 NtCreateFile

import pb

NUMBERS = None          # None = all; e.g. [0x18, 0x50] once discovered
DETAIL_FIRST = 20
SUMMARY_EVERY = 200

total = 0
counts = {}             # number -> event count (entry+exit)

pb.on_syscall(numbers=NUMBERS)

def on_syscall(evt):
    # evt: {number, phase (0=entry, 1=exit), tid, args[6], retval}
    global total
    total += 1
    num = evt["number"]
    counts[num] = counts.get(num, 0) + 1
    if total <= DETAIL_FIRST:
        if evt["phase"] == 0:
            pb.print("SYSCALL 0x%02x enter tid=%d args=[%s]"
                     % (num, evt["tid"],
                        " ".join("0x%x" % a for a in evt["args"])))
        else:
            pb.print("SYSCALL 0x%02x exit  tid=%d retval=0x%x"
                     % (num, evt["tid"], evt["retval"]))
    if total % SUMMARY_EVERY == 0:
        top = sorted(counts.items(), key=lambda kv: -kv[1])[:12]
        pb.print("syscall_watch: %d events, %d distinct numbers; top: %s"
                 % (total, len(counts),
                    ", ".join("0x%02x=%d" % kv for kv in top)))

def on_unload():
    pb.print("syscall_watch: unloaded (%d events, %d distinct numbers)"
             % (total, len(counts)))
