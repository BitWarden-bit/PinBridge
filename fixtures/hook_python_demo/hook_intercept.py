"""Real-Pin synchronous interception and asynchronous Hook observation."""

import pb


expected_entry_address = 0
expected_return_address = 0
observed_entry = 0
observed_return = 0


def observe_entry(event):
    global observed_entry
    if event["address"] != expected_entry_address:
        raise RuntimeError("hook.entry escaped address filter")
    if event["registers"].get("rcx") != 5:
        raise RuntimeError("unexpected observed entry registers %r" % event["registers"])
    observed_entry += 1
    pb.print(
        "HOOK_ENTRY_OBSERVE_PASS tid=%d address=0x%x"
        % (event["tid"], event["address"])
    )


def observe_return(event):
    global observed_return
    if event["address"] != expected_return_address:
        raise RuntimeError("hook.return escaped address filter")
    if event["return_value"] != 17:
        raise RuntimeError("unexpected observed return %r" % event["return_value"])
    observed_return += 1
    pb.print(
        "HOOK_RETURN_OBSERVE_PASS tid=%d address=0x%x return=%d"
        % (event["tid"], event["address"], event["return_value"])
    )


def verify_observers(event):
    if (observed_entry, observed_return) != (1, 2):
        raise RuntimeError(
            "Hook observers were not exact-once: entry=%d return=%d"
            % (observed_entry, observed_return)
        )
    pb.print("HOOK_OBSERVE_EXACT_ONCE entry=1 return=2")


def intercept_skip(event):
    registers = event["registers"]
    argument_register = "rcx" if "rcx" in registers else "stack0"
    argument = registers.get("rcx", event["arguments"][0])
    if argument not in (5, 6):
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
    global expected_entry_address, expected_return_address
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
    expected_entry_address = skip_address
    expected_return_address = return_address
    observe_entry_id = pb.on(
        "hook.entry", observe_entry, address=skip_address, once=True
    )
    observe_return_id = pb.on(
        "hook.return", observe_return, address=return_address, once=False
    )
    pb.on("process.prepare_fini", verify_observers, once=True)
    entry_id = pb.intercept(
        "hook.entry", intercept_skip, address=skip_address, once=False
    )
    return_id = pb.intercept(
        "hook.return", intercept_return, address=return_address, once=True
    )
    pb.print(
        "HOOK_INTERCEPT_READY entry_id=%d return_id=%d observe=%d/%d entry=0x%x return=0x%x"
        % (
            entry_id,
            return_id,
            observe_entry_id,
            observe_return_id,
            skip_address,
            return_address,
        )
    )
    # Release the target only after every handler is registered, then keep
    # pb_init active long enough for the first Hook to arrive. This is the
    # regression for the per-registration event cursor boundary.
    with open("hook_registration.ready", "wb"):
        pass
    pb.sleep(500)
