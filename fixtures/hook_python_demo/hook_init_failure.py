"""Registers native resources and then fails to verify host quarantine."""

import pb


def never_observe(_event):
    raise RuntimeError("failed plugin Hook observer must never run")


def never_intercept(_event):
    raise RuntimeError("failed plugin Hook interceptor must never run")


def never_break(_event):
    raise RuntimeError("failed plugin breakpoint must never run")


def pb_init():
    main = next((row[3] for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main = main.replace("/", "\\").split("\\")[-1]
    address = pb.resolve_name(main + "!DemoSkip")
    if not address:
        raise RuntimeError("DemoSkip export not found")

    observer_id = pb.on("hook.entry", never_observe, address=address)
    interceptor_id = pb.intercept("hook.entry", never_intercept, address=address)
    breakpoint_id = pb.breakpoint(address, never_break)
    pb.print(
        "HOOK_INIT_FAILURE_ARMED observer=%d interceptor=%d breakpoint=%d address=0x%x"
        % (observer_id, interceptor_id, breakpoint_id, address)
    )
    raise RuntimeError("intentional initialization failure after native registration")
