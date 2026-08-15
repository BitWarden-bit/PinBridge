"""Real-Pin verification of Python-controlled detach and JIT reattach."""

import pb


ATTACHED = False
INSTRUCTION_AFTER_ATTACH = False
THREAD_AFTER_ATTACH = False


def routine_range(entry):
    rows = pb.disasm(entry, 96)
    if not rows:
        raise RuntimeError("failed to disassemble AfterAttachTarget")
    for address, size, _kind, _target, text in rows:
        if text.strip().lower().startswith("ret"):
            return entry, address + size
    raise RuntimeError("AfterAttachTarget ret not found")


def on_instruction(event):
    global INSTRUCTION_AFTER_ATTACH
    if not (TARGET_START <= event["address"] < TARGET_END):
        raise RuntimeError("reattach instrumentation range leaked")
    if ATTACHED and not INSTRUCTION_AFTER_ATTACH:
        INSTRUCTION_AFTER_ATTACH = True
        pb.print("REATTACH_INSTRUCTION_AFTER_ATTACH address=0x%x" % event["address"])


def on_thread_start(event):
    global THREAD_AFTER_ATTACH
    if ATTACHED and not THREAD_AFTER_ATTACH:
        THREAD_AFTER_ATTACH = True
        pb.print("REATTACH_THREAD_AFTER_ATTACH tid=%d" % event["tid"])


def on_detach(event):
    pb.print("REATTACH_DETACHED state=%s" % (pb.pin_state()[0],))
    for _attempt in range(200):
        if pb.pin_attach():
            pb.print("REATTACH_REQUEST_ACCEPTED")
            return
        pb.sleep(5)
    raise RuntimeError("Pin did not accept reattach after detach completion")


def on_attach(event):
    global ATTACHED
    state, status = pb.pin_state()
    if state != "attached" or status != 0:
        raise RuntimeError("bad reattach state: %s status=%d" % (state, status))
    ATTACHED = True
    if pb.write_mem(READY_ADDRESS, (1).to_bytes(4, "little")) != 4:
        raise RuntimeError("failed to release reattach target")
    pb.print("REATTACH_ATTACHED state=%s" % state)


def pb_init():
    global TARGET_START, TARGET_END, READY_ADDRESS
    main_path = next((row[3] for row in pb.modules() if row[2]), None)
    if main_path is None:
        raise RuntimeError("main module not found")
    main = main_path.replace("/", "\\").split("\\")[-1]
    target = pb.resolve_name(main + "!AfterAttachTarget")
    READY_ADDRESS = pb.resolve_name(main + "!ReattachReady")
    if not target or not READY_ADDRESS:
        raise RuntimeError("reattach fixture exports not found")
    TARGET_START, TARGET_END = routine_range(target)

    pb.on("pin.detach", on_detach, once=True)
    pb.on("pin.attach", on_attach, once=True)
    pb.on("thread.start", on_thread_start)
    pb.on("instruction", on_instruction)
    pb.instrumentation_set(kinds=["instruction"], ranges=[(TARGET_START, TARGET_END)])

    state, status = pb.pin_state()
    if state != "attached" or status != 0:
        raise RuntimeError("unexpected initial Pin state: %s status=%d" % (state, status))
    pb.print("REATTACH_READY")
    if not pb.pin_attach_supported():
        pb.print("REATTACH_UNSUPPORTED_SAFE platform=windows-jit")
        return
    if not pb.pin_detach():
        raise RuntimeError("Pin rejected detach request")
    pb.print("REATTACH_DETACH_REQUESTED")
