"""Persistent real exception callback used to inspect the UI workspace."""

import pb


UI_DEMO_EXCEPTION = 0xC0000005


def observe_exception(event):
    pb.print(
        "UI_MONITOR code=0x%08x tid=%d generation=%d"
        % (event["code"] & 0xFFFFFFFF, event["tid"], event["exception_generation"])
    )


def handle_exception(event):
    pb.print(
        "UI_HANDLE code=0x%08x tid=%d address=0x%x"
        % (event["code"], event["tid"], event["address"])
    )
    # No context patch: the target's native SEH handler remains responsible.
    return None


def pb_init():
    pb.on("exception", observe_exception)
    decision_id = pb.intercept(
        "exception.handle",
        handle_exception,
        codes=[UI_DEMO_EXCEPTION],
        once=False,
    )
    pb.print("UI_EXCEPTION_READY id=%d" % decision_id)
