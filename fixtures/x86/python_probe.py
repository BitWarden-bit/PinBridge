"""Real ia32 Pin/Python smoke test with an exact Python breakpoint."""

import pb


def on_next_instruction(event):
    if event.get("arch") != "x86" or event.get("pointer_width") != 4:
        raise RuntimeError("not an x86 stop event: %r" % event)
    registers = event.get("registers") or {}
    if "eip" not in registers or "esp" not in registers:
        raise RuntimeError("x86 context is incomplete: %r" % registers)
    if registers["eip"] != event["address"]:
        raise RuntimeError(
            "x86 breakpoint context drifted: address=0x%x eip=0x%x"
            % (event["address"], registers["eip"])
        )
    pb.print(
        "X86_PYTHON_BREAKPOINT_PASS tid=%d address=0x%x eip=0x%x"
        % (event["tid"], event["address"], registers["eip"])
    )
    return "resume"


def pb_init():
    if not pb.is_stopped():
        raise RuntimeError("x86 target was not stopped at its entry point")
    tid, entry = pb.hit()
    if tid is None or not entry:
        raise RuntimeError("missing x86 entry breakpoint context")
    stopped_eip = pb.get_reg(tid, "eip")
    if stopped_eip is None or not stopped_eip:
        raise RuntimeError("missing saved x86 EIP")
    if stopped_eip != entry:
        raise RuntimeError(
            "x86 entry context drifted: entry=0x%x eip=0x%x" % (entry, stopped_eip)
        )
    if pb.get_reg(tid, "rip") is not None:
        raise RuntimeError("x64 RIP unexpectedly resolved in an x86 session")

    # 0x40 is INC EAX in legacy 32-bit mode but a REX prefix in long mode.
    # Temporarily decode this mode-sensitive pair at the stopped entry point,
    # then restore the target before it executes.
    original = pb.read_mem(stopped_eip, 2)
    if original is None or len(original) != 2:
        raise RuntimeError("could not read x86 entry bytes")
    original = bytes(original)
    try:
        if not pb.write_mem(stopped_eip, b"\x40\xc3"):
            raise RuntimeError("could not install x86 decode probe")
        mode_rows = pb.disasm(stopped_eip, 2) or []
        if not mode_rows or mode_rows[0][1] != 1:
            raise RuntimeError("x86 decoder used long mode: %r" % (mode_rows,))
        text = mode_rows[0][4].lower().replace(" ", "")
        if "inc" not in text or "eax" not in text:
            raise RuntimeError("x86 mode-sensitive decode mismatch: %r" % (mode_rows[0],))
    finally:
        if not pb.write_mem(stopped_eip, original):
            raise RuntimeError("could not restore x86 entry bytes")

    rows = pb.disasm(stopped_eip, 4) or []
    if len(rows) < 2:
        raise RuntimeError("could not decode x86 entry instructions")
    flow_target = rows[0][3]
    next_address = flow_target if flow_target else rows[1][0]
    pb.breakpoint(next_address, on_next_instruction, once=True)
    pb.print(
        "X86_PYTHON_READY control_port=%d entry=0x%x eip=0x%x breakpoint=0x%x"
        % (pb.control_port(), entry, stopped_eip, next_address)
    )
