"""Real-Pin verification for named lifecycle subscriptions."""

import pb


def process_start(event):
    pb.print("LIFECYCLE_PROCESS_START phase=%s" % event["phase"])


def thread_start(event):
    pb.print(
        "LIFECYCLE_THREAD_START tid=%d ip=0x%x flags=%d"
        % (event["tid"], event["ip"], event["flags"])
    )


def thread_exit(event):
    pb.print(
        "LIFECYCLE_THREAD_EXIT tid=%d ip=0x%x code=%d"
        % (event["tid"], event["ip"], event["exit_code"])
    )


def process_exit(event):
    pb.print(
        "LIFECYCLE_PROCESS_EXIT phase=%s source=%s known=%s"
        % (event["phase"], event["source"], event["exit_code_known"])
    )


def process_prepare_fini(event):
    pb.print(
        "LIFECYCLE_PREPARE_FINI phase=%s had_exit_request=%s trigger=%s"
        % (event["phase"], event["had_exit_request"], event["trigger"])
    )


def pb_init():
    names = pb.event_names()
    required = {
        "process.start",
        "process.exit",
        "process.prepare_fini",
        "thread.start",
        "thread.exit",
    }
    if not required.issubset(set(names)):
        raise RuntimeError("missing event names: %r" % sorted(required - set(names)))
    pb.on("process.start", process_start, once=True)
    pb.on("thread.start", thread_start)
    pb.on("thread.exit", thread_exit)
    pb.on("process.exit", process_exit, once=True)
    pb.on("process.prepare_fini", process_prepare_fini, once=True)
    pb.print("LIFECYCLE_READY")
