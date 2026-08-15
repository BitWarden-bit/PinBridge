"""Real-Pin verification of Python-owned exception context recovery."""

import pb


EXCEPTION_CODE = 0xC0000005
named_exception_count = 0
context_exception_count = 0
legacy_exception_count = 0


def check_observed_exception(event, source):
    if (event["code"] & 0xFFFFFFFF) != EXCEPTION_CODE:
        raise RuntimeError("%s saw unexpected exception code 0x%x" % (source, event["code"]))
    if event["exception_generation"] <= 0:
        raise RuntimeError("%s saw no native exception generation" % source)


def observe_exception(event):
    global named_exception_count
    check_observed_exception(event, "named exception")
    named_exception_count += 1
    pb.print("EXCEPTION_OBSERVE_NAMED generation=%d" % event["exception_generation"])


def observe_context(event):
    global context_exception_count
    if event["reason"] != 4:
        return
    if (event["info"] & 0xFFFFFFFF) != EXCEPTION_CODE:
        raise RuntimeError("context observer saw unexpected info 0x%x" % event["info"])
    if event["exception_generation"] <= 0:
        raise RuntimeError("context observer saw no native exception generation")
    context_exception_count += 1
    pb.print("EXCEPTION_OBSERVE_CONTEXT generation=%d" % event["exception_generation"])


def on_exception(event):
    global legacy_exception_count
    check_observed_exception(event, "legacy exception")
    legacy_exception_count += 1
    pb.print("EXCEPTION_OBSERVE_LEGACY generation=%d" % event["exception_generation"])


def verify_observers(event):
    if named_exception_count != 1 or context_exception_count != 1 or legacy_exception_count != 1:
        raise RuntimeError(
            "exception observers were not exact-once: named=%d context=%d legacy=%d"
            % (named_exception_count, context_exception_count, legacy_exception_count)
        )
    pb.print("EXCEPTION_OBSERVE_COUNTS named=1 context=1 legacy=1")


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
    pb.on_exception(codes=[EXCEPTION_CODE])
    pb.on("exception", observe_exception)
    pb.on("context.change", observe_context)
    pb.on("process.prepare_fini", verify_observers, once=True)
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
