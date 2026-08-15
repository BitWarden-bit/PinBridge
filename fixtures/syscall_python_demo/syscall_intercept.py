"""Real-Pin syscall.entry/syscall.exit synchronous verification."""

import pb

exit_hits = 0
exit_decision_id = 0
named_entry_hits = 0
named_exit_hits = 0
legacy_entry_hits = 0
legacy_exit_hits = 0
observed_number = 0
named_generations = set()
legacy_generations = set()


def observe_syscall(event):
    global named_entry_hits, named_exit_hits
    if event["number"] != observed_number:
        raise RuntimeError("named observer escaped native number filter")
    generation = event["syscall_generation"]
    if generation <= 0:
        raise RuntimeError("named observer saw no syscall generation")
    if generation in named_generations:
        raise RuntimeError("named observer received duplicate generation %d" % generation)
    named_generations.add(generation)
    if event["phase"] == "enter":
        named_entry_hits += 1
    elif event["phase"] == "exit":
        named_exit_hits += 1
    else:
        raise RuntimeError("named observer saw invalid phase %r" % event["phase"])


def on_syscall(event):
    global legacy_entry_hits, legacy_exit_hits
    if event["number"] != observed_number:
        raise RuntimeError("legacy observer escaped native number filter")
    generation = event["syscall_generation"]
    if generation <= 0:
        raise RuntimeError("legacy observer saw no syscall generation")
    if generation in legacy_generations:
        raise RuntimeError("legacy observer received duplicate generation %d" % generation)
    legacy_generations.add(generation)
    if event["phase"] == 0:
        legacy_entry_hits += 1
    elif event["phase"] == 1:
        legacy_exit_hits += 1
    else:
        raise RuntimeError("legacy observer saw invalid phase %r" % event["phase"])


def verify_observers(event):
    named_counts = (named_entry_hits, named_exit_hits)
    legacy_counts = (legacy_entry_hits, legacy_exit_hits)
    if named_counts != legacy_counts or min(named_counts) < 2:
        raise RuntimeError(
            "syscall observer counts differ or missed target calls: named=%r legacy=%r"
            % (named_counts, legacy_counts)
        )
    if named_generations != legacy_generations:
        raise RuntimeError("syscall observer generation sets differ")
    expected = named_entry_hits + named_exit_hits
    if len(named_generations) != expected or len(legacy_generations) != expected:
        raise RuntimeError("syscall observer generation was not exact-once")
    pb.print(
        "SYSCALL_OBSERVE_EXACT_ONCE total=%d entry=%d exit=%d"
        % (expected, named_entry_hits, named_exit_hits)
    )


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
    global exit_decision_id, observed_number
    address = pb.resolve_name("ntdll.dll!NtClose")
    if not address:
        raise RuntimeError("NtClose not resolved")
    number = syscall_number(address)
    observed_number = number
    names = pb.decision_names()
    if "syscall.entry" not in names or "syscall.exit" not in names:
        raise RuntimeError("syscall interceptors are not public")
    pb.on_syscall(numbers=[number])
    pb.on("syscall", observe_syscall, numbers=[number])
    pb.on("process.prepare_fini", verify_observers, once=True)
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
