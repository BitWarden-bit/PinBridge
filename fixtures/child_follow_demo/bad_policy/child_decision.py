"""Initializes successfully but conflicts at transactional policy commit."""

import pb


def wrong_decision(_event):
    return {"follow": False}


def pb_init():
    pb.intercept("child.follow", wrong_decision, once=False)
    pb.xed_decode_set(cet=False)
    pb.print("CHILD_POLICY_REPLACEMENT_ARMED")
