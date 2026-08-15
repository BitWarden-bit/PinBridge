"""Real-Pin verification for named lifecycle subscriptions."""

import pb


module_base = None
module_load_count = 0
module_unload_count = 0
legacy_module_base = None
legacy_module_load_count = 0
legacy_module_unload_count = 0


def process_start(event):
    pb.print("LIFECYCLE_PROCESS_START phase=%s" % event["phase"])


def thread_start(event):
    pb.print(
        "LIFECYCLE_THREAD_START tid=%d ip=0x%x flags=%d"
        % (event["tid"], event["ip"], event["flags"])
    )


def thread_exit(event):
    pb.print(
        "LIFECYCLE_THREAD_EXIT tid=%d ip=0x%x code=%d"
        % (event["tid"], event["ip"], event["exit_code"])
    )


def module_load(event):
    global module_base, module_load_count
    name = event.get("name", "").lower()
    if "lifecycle_module_x64.dll" not in name:
        return
    if event["module_generation"] <= 0:
        raise RuntimeError("module load has no native generation")
    module_base = event["base"]
    module_load_count += 1
    pb.print(
        "LIFECYCLE_MODULE_LOAD base=0x%x generation=%d"
        % (module_base, event["module_generation"])
    )


def module_unload(event):
    global module_unload_count
    if module_base is None or event["base"] != module_base:
        return
    if event["module_generation"] <= 0:
        raise RuntimeError("module unload has no native generation")
    module_unload_count += 1
    pb.print(
        "LIFECYCLE_MODULE_UNLOAD base=0x%x generation=%d"
        % (event["base"], event["module_generation"])
    )


def on_module_load(event):
    global legacy_module_base, legacy_module_load_count
    name = event.get("name", "").lower()
    if "lifecycle_module_x64.dll" not in name:
        return
    if event["module_generation"] <= 0:
        raise RuntimeError("legacy module load has no native generation")
    legacy_module_base = event["base"]
    legacy_module_load_count += 1
    pb.print("LIFECYCLE_LEGACY_MODULE_LOAD base=0x%x" % legacy_module_base)


def on_module_unload(event):
    global legacy_module_unload_count
    if legacy_module_base is None or event["base"] != legacy_module_base:
        return
    if event["module_generation"] <= 0:
        raise RuntimeError("legacy module unload has no native generation")
    legacy_module_unload_count += 1
    pb.print("LIFECYCLE_LEGACY_MODULE_UNLOAD base=0x%x" % event["base"])


def process_exit(event):
    pb.print(
        "LIFECYCLE_PROCESS_EXIT phase=%s source=%s known=%s"
        % (event["phase"], event["source"], event["exit_code_known"])
    )


def process_prepare_fini(event):
    if module_load_count != 1 or module_unload_count != 1:
        raise RuntimeError(
            "module callbacks were not exact-once: load=%d unload=%d"
            % (module_load_count, module_unload_count)
        )
    if legacy_module_load_count != 1 or legacy_module_unload_count != 1:
        raise RuntimeError(
            "legacy module callbacks were not exact-once: load=%d unload=%d"
            % (legacy_module_load_count, legacy_module_unload_count)
        )
    pb.print(
        "LIFECYCLE_PREPARE_FINI phase=%s had_exit_request=%s trigger=%s"
        % (event["phase"], event["had_exit_request"], event["trigger"])
    )
    pb.print("LIFECYCLE_MODULE_COUNTS named=1/1 legacy=1/1")


def pb_init():
    names = pb.event_names()
    required = {
        "process.start",
        "process.exit",
        "process.prepare_fini",
        "thread.start",
        "thread.exit",
        "module.load",
        "module.unload",
    }
    if not required.issubset(set(names)):
        raise RuntimeError("missing event names: %r" % sorted(required - set(names)))
    pb.on("process.start", process_start, once=True)
    pb.on("thread.start", thread_start)
    pb.on("thread.exit", thread_exit)
    pb.on("module.load", module_load)
    pb.on("module.unload", module_unload)
    pb.on("process.exit", process_exit, once=True)
    pb.on("process.prepare_fini", process_prepare_fini, once=True)
    pb.print("LIFECYCLE_READY")
