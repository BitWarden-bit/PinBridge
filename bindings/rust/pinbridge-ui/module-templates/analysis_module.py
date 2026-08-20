import pb

# 独立模块脚本：状态会在多个事件回调之间保留。
# 修改为实际的 module!symbol 后热更新模块即可开始分析。
TARGET = None  # 例如 "target.exe!bug_function"
WINDOW_SIZE = 0x400
EVENT_LIMIT = 512

state = {
    "phase": "waiting",
    "armed": False,
    "target": 0,
    "events": [],
}


def try_arm():
    if state["armed"] or not TARGET:
        return
    address = pb.resolve_name(TARGET)
    if not address:
        pb.print(f"[module] waiting for {TARGET}")
        return
    pb.breakpoint(
        address,
        on_bug_region,
        description="进入目标 BUG 区域后检查现场并启动局部指令/内存采集",
    )
    state["armed"] = True
    state["target"] = address
    pb.print(f"[module] armed {TARGET} at {address:#x}")


def pb_init():
    pb.print("[module] loaded; edit TARGET to arm the first stage")
    try_arm()


def on_module_load(event):
    # 目标 DLL 晚加载时继续尝试解析入口。
    try_arm()


def on_bug_region(event):
    tid = event["tid"]
    rip = event["address"]
    rax = pb.get_reg(tid, "rax")
    code = pb.disasm(rip, 16)
    pb.print(f"[module] hit tid={tid} rip={rip:#x} rax={rax!r} ins={len(code)}")

    pb.instrumentation_set(
        kinds=["instruction", "memory"],
        ranges=[(rip, rip + WINDOW_SIZE)],
    )
    state["phase"] = "collecting"
    state["events"].clear()
    return "resume"


def on_event_batch(events, missed):
    if state["phase"] != "collecting":
        return
    state["events"].extend(events)
    if missed:
        pb.print(f"[module] event gap: {missed}")
    if len(state["events"]) >= EVENT_LIMIT:
        pb.instrumentation_clear()
        state["phase"] = "complete"
        pb.print(f"[module] analysis complete: {len(state['events'])} events")


def on_unload():
    pb.instrumentation_clear()
    pb.print("[module] unloaded")
