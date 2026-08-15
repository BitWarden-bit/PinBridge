"""Real-Pin child.follow synchronous decision verification."""

import os

import pb


def decide_child(event):
    follow = os.environ.get("PINBRIDGE_TEST_FOLLOW_CHILD") == "1"
    if "--child" not in event["argv"]:
        raise RuntimeError("unexpected child argv: %r" % event["argv"])
    pb.print(
        "CHILD_DECISION_PASS pid=%d follow=%s argv=%r"
        % (event["pid"], follow, event["argv"])
    )
    return {"follow": follow}


def pb_init():
    if "child.follow" not in pb.decision_names():
        raise RuntimeError("child.follow is not a public decision")
    pb.intercept("child.follow", decide_child, once=True)
    pb.print("CHILD_DECISION_READY")
