"""Real-Pin verification of Python-configured native instrumentation."""

import pb


HITS = 0


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
    generation = pb.instrumentation_set(
        kinds=["instruction"],
        ranges=[(INCLUDED_START, INCLUDED_END)],
    )
    policy = pb.instrumentation_policy()
    if policy is None or policy[0] != ["instruction"]:
        raise RuntimeError("instrumentation policy did not round-trip")
    pb.print(
        "INSTRUMENTATION_POLICY_READY generation=%d included=0x%x-0x%x excluded=0x%x-0x%x"
        % (generation, INCLUDED_START, INCLUDED_END, EXCLUDED_START, EXCLUDED_END)
    )
