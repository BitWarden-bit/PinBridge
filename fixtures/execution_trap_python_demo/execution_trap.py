import pb

target = pb.resolve_name("execution_trap_demo_x64.exe!trap_target")
if not target:
    raise RuntimeError("cannot resolve exported trap_target")

trap_id = None


def on_trap(event):
    if event["id"] != trap_id:
        return
    rip = pb.get_reg(event["tid"], "rip")
    if rip != target or event["address"] != target:
        raise RuntimeError(
            "inexact execution trap: event=0x%x rip=0x%x expected=0x%x"
            % (event["address"], rip or 0, target)
        )
    pb.print(
        "EXECUTION_TRAP_HIT id=%d address=0x%x rip=0x%x stop_generation=%d"
        % (event["id"], event["address"], rip, event["stop_generation"])
    )
    if not pb.resume():
        raise RuntimeError("execution trap callback could not resume target")


pb.on("execution.trap", on_trap)
trap_id = pb.execution_trap(target, target + 1, once=True)
with open("execution_trap.ready", "w", encoding="ascii") as ready:
    ready.write(str(trap_id))
pb.print("EXECUTION_TRAP_READY id=%d address=0x%x" % (trap_id, target))
