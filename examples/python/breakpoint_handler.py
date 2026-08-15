"""Bound breakpoint callback example for the in-agent Python host.

The target is already stopped when ``on_target_breakpoint`` runs.  Returning
an action lets the host resume/step exactly once after every matching plugin
has finished.  Return None (or "stay") while inspecting interactively.
"""

import pb


TARGET = "target.exe!Function"
breakpoint_id = None


def on_target_breakpoint(event):
    regs = event["registers"]
    ip_name = "rip" if event["pointer_width"] == 8 else "eip"
    sp_name = "rsp" if event["pointer_width"] == 8 else "esp"
    ip = regs.get(ip_name, event["address"])
    sp = regs.get(sp_name, 0)

    pb.print(
        "breakpoint #%d hit=%d tid=%d ip=0x%x sp=0x%x"
        % (event["id"], event["hits"], event["tid"], ip, sp)
    )

    # The target remains stopped throughout this callback.  Read anything
    # needed for the decision and use pb.set_reg/pb.write_mem to patch it.
    stack = pb.read_mem(sp, 0x80) if sp else None
    if stack is not None:
        pb.print("captured %d stack bytes" % len(stack))

    # Other valid results: "stay", "step_into", "step_over", None.
    return "resume"


def pb_init():
    global breakpoint_id
    address = pb.resolve_name(TARGET)
    if address is None:
        pb.print("cannot resolve " + TARGET)
        return
    breakpoint_id = pb.breakpoint(
        address,
        on_target_breakpoint,
        once=False,
        thread_id=None,
    )
    pb.print("bound breakpoint #%d at 0x%x" % (breakpoint_id, address))


def on_unload():
    # Plugin unload releases all bound breakpoints automatically.  Explicit
    # removal is useful when a running plugin no longer needs one:
    # pb.breakpoint_remove(breakpoint_id)
    pb.print("breakpoint plugin unloaded")
