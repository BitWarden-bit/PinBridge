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
    pb.print("LIFECYCLE_PROCESS_EXIT phase=%s" % event["phase"])


def pb_init():
    names = pb.event_names()
    required = {"process.start", "process.exit", "thread.start", "thread.exit"}
    if not required.issubset(set(names)):
        raise RuntimeError("missing event names: %r" % sorted(required - set(names)))
    pb.on("process.start", process_start, once=True)
    pb.on("thread.start", thread_start)
    pb.on("thread.exit", thread_exit)
    pb.on("process.exit", process_exit, once=True)
    pb.print("LIFECYCLE_READY")
