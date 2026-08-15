"""Real-Pin syscall.entry/syscall.exit synchronous verification."""

import pb

exit_hits = 0
exit_decision_id = 0


def intercept_entry(event):
    pb.print(
        "SYSCALL_ENTRY_INTERCEPT_PASS number=0x%x original_arg0=0x%x"
        % (event["number"], event["arguments"][0])
    )
    return {"arguments": [0]}


def intercept_exit(event):
    global exit_hits, exit_decision_id
    exit_hits += 1
    pb.print(
        "SYSCALL_EXIT_INTERCEPT_PASS hit=%d number=0x%x original=0x%x"
        % (exit_hits, event["number"], event["return_value"])
    )
    if exit_hits == 2:
        pb.unintercept(exit_decision_id)
        return {"return_value": 0xC0000022}
    return None


def syscall_number(address):
    data = bytes(pb.read_mem(address, 32) or [])
    for index in range(0, len(data) - 4):
        if data[index] == 0xB8:
            return int.from_bytes(data[index + 1:index + 5], "little")
    raise RuntimeError("mov eax, syscall_number not found: %r" % data)


def pb_init():
    global exit_decision_id
    address = pb.resolve_name("ntdll.dll!NtClose")
    if not address:
        raise RuntimeError("NtClose not resolved")
    number = syscall_number(address)
    names = pb.decision_names()
    if "syscall.entry" not in names or "syscall.exit" not in names:
        raise RuntimeError("syscall interceptors are not public")
    entry_id = pb.intercept(
        "syscall.entry", intercept_entry, numbers=[number], once=True
    )
    exit_id = pb.intercept(
        "syscall.exit", intercept_exit, numbers=[number]
    )
    exit_decision_id = exit_id
    pb.print(
        "SYSCALL_INTERCEPT_READY number=0x%x entry_id=%d exit_id=%d"
        % (number, entry_id, exit_id)
    )
