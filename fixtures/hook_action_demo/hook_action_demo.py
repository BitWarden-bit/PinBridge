"""Native Hook action demo: capture and change an API argument and return."""

import pb

entry = 0
ret = 0
arg_reg = ""


def pb_init():
    global entry, ret, arg_reg
    main = next((row[3] for row in pb.modules() if row[2]), "")
    main = main.rsplit("\\", 1)[-1].rsplit("/", 1)[-1]
    entry = pb.resolve_name(main + "!DemoApi") if main else 0
    if not entry:
        pb.print("hook_action_demo: DemoApi export not found")
        return

    rows = pb.disasm(entry, 32) or []
    for address, _size, _kind, _target, text in rows:
        if text.strip().lower().startswith("ret"):
            ret = address
            break
    if not ret:
        pb.print("hook_action_demo: DemoApi ret not found")
        return

    if not pb.hook_set(entry) or not pb.hook_set(ret):
        pb.print("hook_action_demo: hook_set failed")
        return

    # The first call argument is RCX on Win64 and [ESP+4] on ia32.
    try:
        pb.hook_rules_clear()
        arg_reg = "rcx"
        if not pb.hook_rule(entry, arg_reg, 20):
            raise RuntimeError("rcx rule rejected")
        return_reg = "rax"
    except Exception:
        pb.hook_rules_clear()
        arg_reg = "stack0"
        if not pb.hook_rule(entry, arg_reg, 20):
            pb.print("hook_action_demo: argument rule failed")
            return
        return_reg = "eax"

    if not pb.hook_rule(ret, return_reg, 0x1234):
        pb.print("hook_action_demo: return rule failed")
        return
    pb.watch(["hook"], range=(entry, ret + 1), batch=64)
    pb.print("HOOK_DEMO_ARMED entry=0x%x ret=0x%x arg=%s->20 return=%s->0x1234" %
             (entry, ret, arg_reg, return_reg))


def on_event_batch(events, missed):
    for event in events:
        if event["kind_name"] == "hook_return" and event["addr"] == ret:
            pb.print("HOOK_RETURN original=0x%x final=0x1234" % event["a0"])
        elif event["addr"] == entry:
            original = event["a4"] if arg_reg == "stack0" else event["a0"]
            pb.print("HOOK_ARG original=%d modified=20" % original)


def on_unload():
    pb.hook_rules_clear()
    pb.hook_clear()
