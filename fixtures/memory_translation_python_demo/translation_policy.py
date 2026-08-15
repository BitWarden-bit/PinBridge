"""Real-Pin verification of Python-configured native address translation."""

import pb


def routine_range(entry):
    rows = pb.disasm(entry, 64)
    if not rows:
        raise RuntimeError("failed to disassemble ReadMappedSource")
    for address, size, _kind, _target, text in rows:
        if text.strip().lower().startswith("ret"):
            return entry, address + size
    raise RuntimeError("ReadMappedSource ret not found")


def pb_init():
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    module = main.replace("/", "\\").split("\\")[-1]
    source = pb.resolve_name(module + "!SourceValue")
    backing = pb.resolve_name(module + "!BackingValue")
    reader = pb.resolve_name(module + "!ReadMappedSource")
    if not source or not backing or not reader:
        raise RuntimeError("fixture exports not found")
    reader_start, reader_end = routine_range(reader)
    generation = pb.memory_translation_set(
        [(source, source + 8, backing)],
        instruction_ranges=[(reader_start, reader_end)],
        operations=["load"],
    )
    policy = pb.memory_translation_policy()
    if policy is None or policy[0] != [(source, source + 8, backing)]:
        raise RuntimeError("memory translation policy did not round-trip")
    if policy[3] != ["load"] or policy[4]:
        raise RuntimeError("memory translation selectors did not round-trip")
    pb.print(
        "MEMORY_TRANSLATION_POLICY_READY generation=%d source=0x%x backing=0x%x reader=0x%x-0x%x"
        % (generation, source, backing, reader_start, reader_end)
    )
