"""Compiles, registers a decision, then fails during replacement init."""

import pb


def wrong_decision(_event):
    return {"follow": False}


def pb_init():
    pb.intercept("child.follow", wrong_decision, once=False)
    pb.print("CHILD_RUNTIME_REPLACEMENT_ARMED")
    raise RuntimeError("intentional replacement initialization failure")
