"""Synchronous Hook action smoke test.

The rule is compiled into the native agent. Python only installs it before
the target resumes; the application-thread callback changes NtClose's RCX
before the real instruction executes.
"""

import pb

MODULE = "ntdll.dll"
TARGET = "NtClose"
target_addr = 0


def pb_init():
    global target_addr
    target_addr = pb.resolve_name("%s!%s" % (MODULE, TARGET)) or 0
    if not target_addr:
        pb.print("hook_modify: target export missing")
        return
    if not pb.hook_set(target_addr):
        pb.print("hook_modify: hook_set failed")
        return
    if not pb.hook_rule(target_addr, "rcx", 0):
        pb.print("hook_modify: hook_rule failed")
        return
    pb.watch(["hook"], range=(target_addr, target_addr + 1), batch=64)
    pb.print("hook_modify: armed %s!%s at 0x%x, RCX -> 0" %
             (MODULE, TARGET, target_addr))


def on_event_batch(events, missed):
    for event in events:
        if event["addr"] == target_addr:
            # The event is intentionally the pre-action snapshot: a0 is the
            # original handle, while the live CONTEXT has already been set to 0.
            pb.print("HOOK_ACTION %s original_rcx=0x%x tid=%d" %
                     (TARGET, event["a0"], event["tid"]))


def on_unload():
    pb.hook_rules_clear()
    pb.hook_clear()
