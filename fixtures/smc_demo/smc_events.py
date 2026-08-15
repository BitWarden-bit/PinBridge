"""Real-Pin verification for the lazy high-priority SMC subscription."""

import pb


def code_changed(event):
    start = event["trace_start"]
    end = event["trace_end"]
    if not start or end < start:
        raise RuntimeError("invalid SMC range: %r" % event)
    pb.print("SMC_EVENT_PASS start=0x%x end=0x%x" % (start, end))


def pb_init():
    if "code.smc" not in pb.event_names():
        raise RuntimeError("code.smc is not a public event")
    pb.on("code.smc", code_changed, once=True)
    pb.print("SMC_READY")
