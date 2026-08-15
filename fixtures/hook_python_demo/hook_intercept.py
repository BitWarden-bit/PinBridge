"""Real-Pin synchronous hook.entry verification."""

import pb


def intercept_skip(event):
    registers = event["registers"]
    argument_register = "rcx" if "rcx" in registers else "stack0"
    argument = registers.get("rcx", event["arguments"][0])
    if argument != 5:
        raise RuntimeError("unexpected argument %r" % argument)
    pb.print(
        "HOOK_ENTRY_INTERCEPT_PASS tid=%d address=0x%x argument=%d source=%s"
        % (event["tid"], event["address"], argument, argument_register)
    )
    return {"action": "return", "return_value": 0x1234}


def intercept_return(event):
    original = event["registers"].get("rax", event["registers"].get("eax"))
    if original != 17:
        raise RuntimeError("unexpected original return %r" % original)
    pb.print(
        "HOOK_RETURN_INTERCEPT_PASS tid=%d address=0x%x original=%d"
        % (event["tid"], event["address"], original)
    )
    return {"return_value": 0x5678}


def pb_init():
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main = main.replace("/", "\\").split("\\")[-1]
    skip_address = pb.resolve_name(main + "!DemoSkip")
    return_entry = pb.resolve_name(main + "!DemoReturn")
    if not skip_address or not return_entry:
        raise RuntimeError("fixture exports not found")
    return_address = next(
        (
            address
            for address, _size, _kind, _target, text in pb.disasm(return_entry, 32)
            if text.strip().lower().startswith("ret")
        ),
        None,
    )
    if not return_address:
        raise RuntimeError("DemoReturn ret not found")
    names = pb.decision_names()
    if "hook.entry" not in names or "hook.return" not in names:
        raise RuntimeError("Hook interceptors are not public")
    entry_id = pb.intercept(
        "hook.entry", intercept_skip, address=skip_address, once=True
    )
    return_id = pb.intercept(
        "hook.return", intercept_return, address=return_address, once=True
    )
    pb.print(
        "HOOK_INTERCEPT_READY entry_id=%d return_id=%d entry=0x%x return=0x%x"
        % (entry_id, return_id, skip_address, return_address)
    )
