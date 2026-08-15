"""Keeps one native decode policy active while a replacement is staged."""

import pb


def pb_init():
    pb.xed_decode_set(cet=True)
    pb.print("CHILD_POLICY_GUARD_READY")
