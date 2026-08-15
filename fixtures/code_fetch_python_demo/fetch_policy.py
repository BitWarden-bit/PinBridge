"""Real-Pin verification of Python-provided native instruction bytes."""

import pb


def routine_bytes(entry):
    rows = pb.disasm(entry, 96)
    if not rows:
        raise RuntimeError("failed to disassemble fixture routine")
    for address, size, _kind, _target, text in rows:
        if text.strip().lower().startswith("ret"):
            length = address + size - entry
            data = pb.read_mem(entry, length)
            if data is None or len(data) != length:
                raise RuntimeError("failed to read replacement routine bytes")
            return bytes(data)
    raise RuntimeError("fixture routine ret not found")


def pb_init():
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    module = main.replace("/", "\\").split("\\")[-1]
    original = pb.resolve_name(module + "!OriginalFunction")
    replacement = pb.resolve_name(module + "!ReplacementFunction")
    if not original or not replacement:
        raise RuntimeError("fixture exports not found")

    replacement_bytes = routine_bytes(replacement)
    generation = pb.code_fetch_set([(original, replacement_bytes)])
    policy = pb.code_fetch_policy()
    if (
        policy is None
        or len(policy) != 1
        or policy[0][0] != original
        or bytes(policy[0][1]) != replacement_bytes
    ):
        raise RuntimeError("code fetch policy did not round-trip")
    pb.print(
        "CODE_FETCH_POLICY_READY generation=%d address=0x%x bytes=%d"
        % (generation, original, len(replacement_bytes))
    )
