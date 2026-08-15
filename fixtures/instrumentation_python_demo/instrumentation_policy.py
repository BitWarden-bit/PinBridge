"""Real-Pin verification of Python-configured native instrumentation."""

import pb


HITS = 0
LIFECYCLE_HITS = {}


def routine_range(entry):
    rows = pb.disasm(entry, 96)
    if not rows:
        raise RuntimeError("failed to disassemble fixture routine")
    for address, size, _kind, _target, text in rows:
        if text.strip().lower().startswith("ret"):
            return entry, address + size
    raise RuntimeError("fixture routine ret not found")


def on_instruction(event):
    global HITS
    address = event["address"]
    if EXCLUDED_START <= address < EXCLUDED_END:
        raise RuntimeError("native range filter leaked excluded address 0x%x" % address)
    if not (INCLUDED_START <= address < INCLUDED_END):
        raise RuntimeError("native range filter leaked address 0x%x" % address)
    HITS += 1
    if HITS == 1:
        pb.print("INSTRUMENTATION_NATIVE_HIT address=0x%x" % address)


def on_lifecycle(event):
    address = event["address"]
    if not (INCLUDED_START <= address < INCLUDED_END):
        raise RuntimeError(
            "%s native range filter leaked address 0x%x" % (event["type"], address)
        )
    event_type = event["type"]
    LIFECYCLE_HITS[event_type] = LIFECYCLE_HITS.get(event_type, 0) + 1
    if LIFECYCLE_HITS[event_type] == 1:
        pb.print(
            "INSTRUMENTATION_LIFECYCLE_HIT type=%s address=0x%x generation=%d"
            % (event_type, address, event["policy_generation"])
        )


def pb_init():
    global INCLUDED_START, INCLUDED_END, EXCLUDED_START, EXCLUDED_END
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main = main.replace("/", "\\").split("\\")[-1]
    included = pb.resolve_name(main + "!IncludedFunction")
    excluded = pb.resolve_name(main + "!ExcludedFunction")
    if not included or not excluded:
        raise RuntimeError("fixture exports not found")
    INCLUDED_START, INCLUDED_END = routine_range(included)
    EXCLUDED_START, EXCLUDED_END = routine_range(excluded)
    pb.on("instruction", on_instruction)
    for event_name in (
        "trace.instrument",
        "routine.instrument",
        "basic_block.instrument",
    ):
        pb.on(event_name, on_lifecycle)
    generation = pb.instrumentation_set(
        kinds=[
            "instruction",
            "trace.instrument",
            "routine.instrument",
            "basic_block.instrument",
        ],
        ranges=[(INCLUDED_START, INCLUDED_END)],
    )
    policy = pb.instrumentation_policy()
    expected_kinds = [
        "instruction",
        "trace.instrument",
        "routine.instrument",
        "basic_block.instrument",
    ]
    if policy is None or policy[0] != expected_kinds:
        raise RuntimeError("instrumentation policy did not round-trip")
    pb.print(
        "INSTRUMENTATION_POLICY_READY generation=%d included=0x%x-0x%x excluded=0x%x-0x%x"
        % (generation, INCLUDED_START, INCLUDED_END, EXCLUDED_START, EXCLUDED_END)
    )
