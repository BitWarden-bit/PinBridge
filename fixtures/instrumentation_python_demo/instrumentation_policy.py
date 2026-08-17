"""Real-Pin verification of Python-configured native instrumentation."""

import pb


HITS = 0
BATCH_HITS = 0
LIFECYCLE_HITS = {}
POLICY_GENERATION = 0


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
    if event["size"] <= 0 or event["next_address"] != address + event["size"]:
        raise RuntimeError("runtime instruction metadata is inconsistent")
    if event["policy_generation"] != POLICY_GENERATION:
        raise RuntimeError(
            "instruction policy generation %d != %d"
            % (event["policy_generation"], POLICY_GENERATION)
        )
    HITS += 1
    if HITS == 1:
        pb.print(
            "INSTRUMENTATION_NATIVE_HIT address=0x%x size=%d generation=%d"
            % (address, event["size"], event["policy_generation"])
        )


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


def on_event_batch(events, missed):
    global BATCH_HITS
    if missed:
        raise RuntimeError("runtime instruction batch missed %d events" % missed)
    for event in events:
        address = event["addr"]
        if not (INCLUDED_START <= address < INCLUDED_END):
            raise RuntimeError("batch range filter leaked address 0x%x" % address)
        if event["a0"] <= 0 or event["a7"] != POLICY_GENERATION:
            raise RuntimeError("raw runtime instruction metadata is inconsistent")
        BATCH_HITS += 1
        if BATCH_HITS == 1:
            pb.print(
                "INSTRUMENTATION_BATCH_HIT address=0x%x size=%d generation=%d"
                % (address, event["a0"], event["a7"])
            )


def pb_init():
    global INCLUDED_START, INCLUDED_END, EXCLUDED_START, EXCLUDED_END
    global POLICY_GENERATION
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
    pb.watch(["exec"], range=(INCLUDED_START, INCLUDED_END), batch=64)
    POLICY_GENERATION = pb.instrumentation_set(
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
        % (POLICY_GENERATION, INCLUDED_START, INCLUDED_END, EXCLUDED_START, EXCLUDED_END)
    )
