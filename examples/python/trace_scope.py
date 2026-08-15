"""Script-defined trace scope and trigger template.

This file is intentionally a normal plugin: edit the constants, load it at
the entry stop, then resume the target. The recorder receives only the
selected module RVAs and the thread that hit the trigger breakpoint.

Set TRACE_DYNAMIC_SCOPE=1 to detect a branch from the selected module into a
private executable VirtualAlloc region and atomically extend the native trace
for that region. The extension is deliberately bounded and marker-tagged.
"""

import os
import ntpath

import pb


TARGET_MODULE = os.environ.get("TRACE_MODULE", "crypto.exe")
# Comma-separated RVA ranges, e.g. TRACE_RANGES=0x6000-0x6800,0x7000-0x7100
RANGE_TEXT = os.environ.get("TRACE_RANGES", "0x6000-0x6800")
TRIGGER_RVA = int(os.environ.get("TRACE_TRIGGER_RVA", "0"), 0)
TRACE_PATH = os.environ.get("TRACE_OUTPUT", r"C:\tmp\scoped.pbtr")
TRACE_KINDS = [x.strip() for x in os.environ.get(
    "TRACE_KINDS", "exec,memory,registers,branch").split(",") if x.strip()]
# `hit` binds a breakpoint/sole paused thread; use `all` or an explicit Pin id
# to widen the scope deliberately.
THREAD_MODE = os.environ.get("TRACE_THREAD", "hit")
MAX_EVENTS = int(os.environ.get("TRACE_MAX_EVENTS", "0"), 0)
DYNAMIC_SCOPE = os.environ.get("TRACE_DYNAMIC_SCOPE", "0").lower() in ("1", "true", "yes")
MAX_DYNAMIC_RANGES = int(os.environ.get("TRACE_MAX_DYNAMIC_RANGES", "4"), 0)
MAX_DYNAMIC_REGION = int(os.environ.get("TRACE_MAX_DYNAMIC_REGION", "0x1000000"), 0)

_ranges = []
_trigger_id = None
_recording = False
_seen_events = 0
_dynamic_ranges = []


def _parse_ranges():
    ranges = []
    for item in RANGE_TEXT.split(","):
        lo, hi = item.split("-", 1)
        lo, hi = int(lo, 0), int(hi, 0)
        if lo >= hi:
            raise ValueError("invalid RVA range: " + item)
        ranges.append((lo, hi))
    return ranges


def _find_module(name):
    wanted = name.lower()
    for base, end, is_main, path in pb.modules():
        if ntpath.basename(path).lower() == wanted or path.lower() == wanted:
            return base, end, bool(is_main), path
    return None


def _resolve_ranges():
    module = _find_module(TARGET_MODULE)
    if module is None:
        raise RuntimeError("module is not loaded: " + TARGET_MODULE)
    base, _end, _is_main, path = module
    rvas = _parse_ranges()
    resolved = [(base + lo, base + hi) for lo, hi in rvas]
    for lo, hi in resolved:
        if hi <= lo:
            raise ValueError("resolved range is empty")
    return path, resolved


def _resolve_threads(trigger_tid=None):
    """Resolve the native recorder allowlist without silently widening it."""
    mode = THREAD_MODE.strip().lower()
    if mode == "all":
        return []
    if mode not in ("hit", "current"):
        try:
            return [int(mode, 0)]
        except ValueError:
            raise ValueError("TRACE_THREAD must be all, hit, current, or a Pin thread id")
    if trigger_tid is not None:
        return [int(trigger_tid)]
    hit_tid, _hit_addr = pb.hit()
    if hit_tid is not None:
        return [int(hit_tid)]
    stopped = pb.threads()
    if len(stopped) == 1:
        return [int(stopped[0])]
    raise RuntimeError(
        "cannot infer one paused thread; set TRACE_THREAD=all or an explicit Pin thread id"
    )


def _arm(thread_id=None):
    global _ranges, _recording, _dynamic_ranges
    path, _ranges = _resolve_ranges()
    _dynamic_ranges = []
    threads = _resolve_threads(thread_id)
    if not pb.trace_start_spec(TRACE_PATH, TRACE_KINDS, _ranges, threads):
        raise RuntimeError("trace_start_spec refused")
    _recording = True
    pb.print("trace_scope: armed module=%s ranges=%s threads=%s output=%s" %
             (path, _ranges, threads or "all", TRACE_PATH))


def _contains(address):
    return any(lo <= address < hi for lo, hi in _ranges)


def _maybe_extend_from_branch(events):
    """Add private executable regions reached by an in-scope branch.

    Branch delivery is asynchronous, so this is a boundary policy rather
    than a claim of zero missing instructions. The native extension itself is
    atomic and marker-tagged; a future hard-stop callback can close the gap.
    """
    if not DYNAMIC_SCOPE or not _recording:
        return
    for event in events:
        if event.get("kind_name") != "branch_edge" or not event.get("a1"):
            continue
        source = event.get("addr", 0)
        target = event.get("a0", 0)
        if not _contains(source) or target == 0 or _contains(target):
            continue
        if len(_dynamic_ranges) >= MAX_DYNAMIC_RANGES:
            return
        region = pb.memory_region(target)
        if region is None:
            continue
        base, size, _allocation, protect, state, kind = region
        # MEM_COMMIT + MEM_PRIVATE + PAGE_EXECUTE*; do not widen into DLLs.
        if state != 0x1000 or kind != 0x20000 or not (protect & 0xF0):
            continue
        size = min(size, MAX_DYNAMIC_REGION)
        if size <= 0 or any(base == lo for lo, _hi in _dynamic_ranges):
            continue
        end = base + size
        was_stopped = pb.is_stopped()
        if not was_stopped:
            pb.stop()
        if pb.trace_extend([(base, end)]):
            _dynamic_ranges.append((base, end))
            _ranges.append((base, end))
            pb.print("trace_scope: dynamically added private code 0x%x-0x%x" %
                     (base, end))
        if not was_stopped:
            pb.resume()


def _stop():
    global _recording
    if _recording:
        recorded, dropped = pb.trace_stop()
        _recording = False
        pb.print("trace_scope: stopped recorded=%d dropped=%d" %
                 (recorded, dropped))


def pb_init():
    global _trigger_id, _ranges
    _path, _ranges = _resolve_ranges()  # fail early before target resumes
    if DYNAMIC_SCOPE:
        lo = min(item[0] for item in _ranges)
        hi = max(item[1] for item in _ranges)
        # Observe branch targets from the selected module only; the native
        # recorder still owns the precise multi-range/thread filtering.
        pb.watch(["exec", "branch"], range=(lo, hi), batch=1024)
    if TRIGGER_RVA:
        module = _find_module(TARGET_MODULE)
        _trigger_id = pb.bp_set(module[0] + TRIGGER_RVA)
        pb.print("trace_scope: trigger bp=%s" % _trigger_id)
    else:
        # Entry-stop mode already gives us a deterministic thread boundary.
        _arm()


def on_bp_hit(evt):
    if _trigger_id is None or evt.get("id") != _trigger_id:
        return
    _arm(evt.get("tid"))
    pb.bp_remove(_trigger_id)
    pb.resume()


def on_event_batch(events, missed):
    global _seen_events
    if not _recording:
        return
    if missed:
        pb.print("trace_scope: main ring missed=%d" % missed)
    _maybe_extend_from_branch(events)
    if MAX_EVENTS <= 0:
        return
    _seen_events += len(events)
    if _seen_events >= MAX_EVENTS:
        _stop()
        pb.stop()


pb.watch(["exec"], batch=1024)
