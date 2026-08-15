"""Real-Pin verification of Python-owned exception context recovery."""

import pb


EXCEPTION_CODE = 0xC0000005


def handle_exception(event):
    if event["code"] != EXCEPTION_CODE:
        raise RuntimeError("unexpected exception code 0x%x" % event["code"])
    target_registers = event["registers"]
    instruction_pointer = "rip" if "rip" in target_registers else "eip"
    patch = {instruction_pointer: RECOVERY_ADDRESS}
    if instruction_pointer == "rip":
        # A direct jump into a Win64 function needs the stack shape normally
        # created by CALL: RSP is eight bytes below the caller's aligned RSP.
        patch["rsp"] = event["from_registers"]["rsp"] - 8
    pb.print(
        "EXCEPTION_HANDLE_PASS tid=%d code=0x%x from=0x%x recovery=0x%x"
        % (
            event["tid"],
            event["code"],
            event["address"],
            RECOVERY_ADDRESS,
        )
    )
    return {"registers": patch}


def pb_init():
    global RECOVERY_ADDRESS
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main = main.replace("/", "\\").split("\\")[-1]
    RECOVERY_ADDRESS = pb.resolve_name(main + "!RecoveryPoint")
    if not RECOVERY_ADDRESS:
        raise RuntimeError("RecoveryPoint export not found")
    if "exception.handle" not in pb.decision_names():
        raise RuntimeError("exception.handle is not public")
    decision_id = pb.intercept(
        "exception.handle",
        handle_exception,
        codes=[EXCEPTION_CODE],
        once=True,
    )
    pb.print(
        "EXCEPTION_INTERCEPT_READY id=%d recovery=0x%x"
        % (decision_id, RECOVERY_ADDRESS)
    )
