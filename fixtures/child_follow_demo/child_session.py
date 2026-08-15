"""Loaded through the followed child's independent control plane."""

import os

import pb


def pb_init():
    control_port = pb.control_port()
    parent_port = pb.parent_control_port()
    if not control_port or not parent_port or control_port == parent_port:
        raise RuntimeError(
            "invalid child session topology: child=%r parent=%r"
            % (control_port, parent_port)
        )
    pb.print(
        "CHILD_SESSION_PYTHON_PASS pid=%d control_port=%d parent_port=%d"
        % (os.getpid(), control_port, parent_port)
    )
    with open("child_control_%d.ready" % os.getpid(), "wb"):
        pass
