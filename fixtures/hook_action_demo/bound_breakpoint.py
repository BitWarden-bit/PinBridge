"""End-to-end fixture for pb.breakpoint(address, callback)."""

import pb

hit_count = 0


def on_demo_api(event):
    global hit_count
    hit_count += 1
    registers = event["registers"]
    if not event["context_complete"]:
        pb.print("BOUND_BP_BAD_CONTEXT")
        return "stay"
    if registers.get("rip") != event["address"]:
        pb.print("BOUND_BP_BAD_IP")
        return "stay"

    # With the deterministic entry gate, the first call is the target's
    # baseline. Validate its exact context but leave it untouched. The second
    # call is the one this fixture intentionally rewrites.
    if hit_count == 1:
        pb.print(
            "BOUND_BP_PRIME id=%d tid=%d rip=0x%x"
            % (event["id"], event["tid"], registers["rip"])
        )
        return "resume"

    # DemoApi(value) returns value + 10. Change RCX so the target observes
    # 0x1234 and exits successfully after this callback resumes it.
    if not pb.set_reg(event["tid"], "rcx", 0x122A):
        pb.print("BOUND_BP_SETREG_FAILED")
        return "stay"

    pb.print(
        "BOUND_BP_HIT id=%d tid=%d rip=0x%x rcx=0x%x"
        % (event["id"], event["tid"], registers["rip"], registers["rcx"])
    )
    pb.breakpoint_remove(event["id"])
    return {"action": "resume"}


def pb_init():
    main = None
    for base, end, is_main, name in pb.modules():
        if is_main:
            main = name.replace("/", "\\").split("\\")[-1]
            break
    if main is None:
        pb.print("BOUND_BP_NO_MAIN")
        return
    address = pb.resolve_name(main + "!DemoApi")
    if address is None:
        pb.print("BOUND_BP_NO_EXPORT")
        return
    bp_id = pb.breakpoint(address, on_demo_api, once=False)
    pb.print("BOUND_BP_READY id=%d address=0x%x" % (bp_id, address))
