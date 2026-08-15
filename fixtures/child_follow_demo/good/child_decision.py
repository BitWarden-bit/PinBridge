"""Successful same-name replacement used by the transactional-load test."""

import os

import pb


def decide_child(event):
    follow = os.environ.get("PINBRIDGE_TEST_FOLLOW_CHILD") == "1"
    if "--child" not in event["argv"]:
        raise RuntimeError("unexpected child argv: %r" % event["argv"])
    control_port = event.get("control_port")
    parent_port = event.get("parent_control_port")
    if follow and (
        not isinstance(control_port, int)
        or control_port <= 0
        or control_port == parent_port
    ):
        raise RuntimeError(
            "invalid independent child control port: child=%r parent=%r"
            % (control_port, parent_port)
        )
    pb.print(
        "CHILD_DECISION_PASS pid=%d follow=%s control_port=%s parent_port=%s argv=%r"
        % (event["pid"], follow, control_port, parent_port, event["argv"])
    )
    return {"follow": follow}


def pb_init():
    pb.intercept("child.follow", decide_child, once=True)
    pb.print("CHILD_REPLACEMENT_READY")
