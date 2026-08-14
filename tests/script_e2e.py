# Multi-plugin script-host E2E (pb API v2): hot-load several Python probes
# into the running agent, prove their callbacks through the `script output`
# ring, then prove multi-plugin isolation on unload.
#
# Steps (numbered prints):
#   1 REJECT      broken script -> CLI exits nonzero, "server status 2" +
#                 SyntaxError text surfaced from the server
#   2 LOAD_A      probe_a: resolve_name(rpc_tick) == info tick, bp_set,
#                 on_bp_hit prints A_HIT once (exact park: evt addr and the
#                 hit thread's rip both equal the bp address), auto-resumes;
#                 on_unload prints A_UNLOAD
#   3 LOAD_B      probe_b: watch(exec, main module range) -> on_event_batch
#                 prints B_BATCH once
#   4 CONCURRENT  `script list` shows both running; output has A_INIT+B_INIT
#   5 CALLBACKS   output has A_HIT + B_BATCH within seconds; counters exec>0
#   6 EXPORTS     `exports pb_rpc_fixture.exe` lists rpc_tick at the info
#                 file's tick_address
#   7 HOOK        hook/hooks/hook_regs counter growth/hookdel/hookclear
#   8 SYSCALL     syscallfilter only 0xFFFF freezes the syscall counter,
#                 syscallfilter all resumes it
#   9 EXCEPTION   fixture's raise_av flag poked via stop/write/resume ->
#                 handled AV per loop iteration; pb.on_exception(0xC0000005)
#                 probe sees EXC_SEEN 0xc0000005 (engines 2/3/4 silenced for
#                 the window: plugin cursors page the 64k ring oldest-first
#                 and a rare context_change event would be evicted under the
#                 default exec flood before the cursor reached it)
#  10 ISOLATION   script off probe_a -> only probe_b left, still
#                 delivering; output shows A_UNLOAD
#  11 UNLOAD_ALL  script off (no arg) -> list empty
#  12 SHUTDOWN    clean exit via the fixture's exit flag (stop -> write ->
#                 resume); agent log carries the on_fini summary ("total=");
#                 no crash_dump.txt
#
# Step 12 does not kill by PID: TerminateProcess never runs Pin fini
# callbacks (verified empirically), so the on_fini summary is only reachable
# through a clean target exit. The fixture exposes g_pinbridge_rpc_exit_flag
# (address in the info file) for exactly that. The PID kill stays as the
# teardown fallback.
#
# crash_dump.txt discipline: the agent's first-chance diagnostic handler
# (diag.rs, marked TEMPORARY) logs EVERY first-chance AV — including the
# fixture's __try/__except-handled ones — so step 9's AV window produces
# benign `CRASH code=0xc0000005 ... access=0x1` records. Step 9 verifies
# every record matches that benign signature (a PIN_CRASH line or any other
# code fails the test) and then removes the file; step 12's absence check
# afterwards still proves no real crash.
#
# Known quirks tolerated here:
#   * python-ready race: `script run` right after the port binds can fail
#     with "python unavailable: python310.dll not loaded" for ~1s -> we wait
#     for the interpreter line in the agent log and retry the first load.
#   * pre-existing agent heap-corruption crash (ntdll+0x5b897, documented
#     ~1/20 but observed much more often under this multi-plugin workload —
#     launcher exits rc=0xC0000374 / -1 mid-run): the WHOLE test is retried
#     once from scratch when the agent process dies (detected via CLI
#     connect failures / launcher exit).
#   * mailbox/tick collision: script ops that ride the mailbox (run/off)
#     wait on the scripting thread while the single-threaded query server
#     serves them, so a concurrent loopback tick RPC makes a CLI call eat
#     its 5s read timeout; the collision self-heals -> cli_script_op /
#     script_run retry within a 45s budget.
import json
import os
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REL = os.path.join(REPO, "bindings", "rust", "target", "release")
CLI = REL + r"\pinbridge-cli.exe"
AGENT = REL + r"\pinbridge_agent.dll"
FIXTURE = os.path.join(REPO, "build", "host-tests", "pb_rpc_fixture.exe")
PIN = os.environ.get("PINBRIDGE_PIN_EXE") or (
    os.environ.get("PIN_ROOT") + r"\intel64\bin\pin.exe" if os.environ.get("PIN_ROOT") else None)
if not PIN:
    raise SystemExit(
        "pin.exe not found: set PINBRIDGE_PIN_EXE to the full path of pin.exe "
        "(or PIN_ROOT to your Pin 3.31 SDK root)")
INFO = REL + r"\script_e2e_rpc_info.txt"
LOG = REL + r"\script_e2e_agent.log"
CRASH_DUMP = REL + r"\crash_dump.txt"
PROBE_A_PATH = REL + r"\script_e2e_probe_a.py"
PROBE_B_PATH = REL + r"\script_e2e_probe_b.py"
PROBE_EXC_PATH = REL + r"\script_e2e_probe_exc.py"
BAD_PROBE = REL + r"\script_e2e_bad.py"
PORT = "9011"

# plugin names are the probe file basenames (the CLI derives them from the path)
NAME_A = "script_e2e_probe_a.py"
NAME_B = "script_e2e_probe_b.py"
NAME_EXC = "script_e2e_probe_exc.py"

# engine kind numbers (agent event.rs / engines.rs set_engine_enabled)
ENGINE_SYSCALL = "5"

PROBE_A = """
import pb

TICK = __TICK__  # baked in by the test from the fixture info file
bp_id = None
hit_done = False

def pb_init():
    global bp_id
    pb.print("A_INIT")
    resolved = pb.resolve_name("pb_rpc_fixture.exe!rpc_tick")
    if resolved != TICK:
        pb.print("A_FAIL resolve_name %s != tick 0x%x" % (hex(resolved) if resolved else "None", TICK))
        return
    bp_id = pb.bp_set(TICK)
    if bp_id is None:
        pb.print("A_FAIL bp_set returned None")

def on_bp_hit(evt):
    global hit_done, bp_id
    tid = evt["tid"]
    if tid < 0:
        return  # manual pause (the test's own stop/write/resume): not ours
    addr = evt["addr"]
    rip = pb.get_reg(tid, "rip")
    if addr != TICK or rip != TICK:
        pb.print("A_FAIL addr=0x%x rip=%s tick=0x%x" % (addr, hex(rip) if rip is not None else "None", TICK))
    elif not hit_done:
        hit_done = True
        pb.print("A_HIT %d 0x%x" % (tid, addr))
    if bp_id is not None:
        pb.bp_remove(bp_id)
        bp_id = None
    pb.resume()

def on_unload():
    pb.print("A_UNLOAD")
"""

PROBE_B = """
import pb

reported = False

def pb_init():
    main = None
    for (low, high, is_main, name) in pb.modules():
        if is_main:
            main = (low, high)
            break
    pb.print("B_INIT")
    if main is None:
        pb.print("B_FAIL no main module")
        return
    pb.watch(kinds=["exec"], range=(main[0], main[1]))
    pb.print("B_WATCH 0x%x-0x%x" % main)

def on_event_batch(events, missed):
    global reported
    if not reported and len(events) > 0:
        reported = True
        pb.print("B_BATCH %d" % len(events))
"""

PROBE_EXC = """
import pb

seen = False

def pb_init():
    pb.on_exception(codes=[0xC0000005])
    pb.print("EXC_INIT")

def on_exception(evt):
    global seen
    if not seen:
        seen = True
        # evt code arrives sign-extended (i32 -> i64 -> u64); mask it back
        pb.print("EXC_SEEN 0x%x 0x%x" % (evt["code"] & 0xFFFFFFFF, evt["rip"]))
"""


class AgentDead(Exception):
    """The agent process died mid-run (known intermittent heap crash)."""


# Current session's launcher process / target pid (for death detection and
# PID-precise cleanup; never a blanket taskkill).
PIN_PROC = None
FIXTURE_PID = None


def note_process_state():
    global PIN_PROC
    if PIN_PROC is not None and PIN_PROC.poll() is not None:
        raise AgentDead(f"launcher exited rc={PIN_PROC.returncode}")


def kill_pid(pid):
    if pid:
        subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
             f"Stop-Process -Id {pid} -Force -ErrorAction SilentlyContinue"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cleanup_zombies():
    # Startup-only sweep for leftovers of previous crashed runs of THIS test
    # (same pattern as tests/Kill-Zombies.ps1). Mid-run cleanup kills by PID.
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
         "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'pb_rpc_fixture.exe' }"
         " | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def run_cli(*args):
    # encoding is explicit: text=True would use the locale (GBK on this
    # machine) and a single non-ASCII byte in the reply kills the reader
    # thread and silently eats stdout/stderr
    return subprocess.run([CLI, *args], capture_output=True,
                          encoding="utf-8", errors="replace")


def cli(*args, retries=5):
    full = ["--port", PORT, *args]
    last_err = ""
    for attempt in range(retries):
        note_process_state()
        proc = run_cli(*full)
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        last_err = (proc.stderr or "").strip()
        if "connect" in last_err.lower():
            note_process_state()  # raises AgentDead when the launcher is gone
        if attempt == retries - 1:
            break
        time.sleep(0.3)
    note_process_state()
    if "connect" in last_err.lower():
        # repeated connect failures while the launcher still claims to run:
        # treat as a wedged agent (same retry policy as a proven death)
        raise AgentDead(f"CLI cannot connect: {last_err}")
    print(f"CLI_FAIL at {' '.join(args)}: {last_err}")
    raise subprocess.CalledProcessError(proc.returncode, full)


def cli_script_op(*args, budget_s=45.0):
    """Script ops that ride the mailbox (run/off) can collide with the
    scripting thread's loopback tick RPC on the single-threaded query
    server: while a script command waits for the host, the host's own tick
    request queues behind it, and a CLI call then surfaces its 5s read
    timeout (rc=1). The collision self-heals once the timeouts drain, so
    retry within a budget instead of failing on the first timeout."""
    deadline = time.time() + budget_s
    last_err = ""
    while True:
        note_process_state()
        proc = run_cli("--port", PORT, *args)
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        last_err = (proc.stderr or "").strip()
        if "connect" in last_err.lower():
            note_process_state()  # raises AgentDead when the launcher is gone
        if time.time() >= deadline:
            break
        time.sleep(0.5)
    note_process_state()
    print(f"CLI_FAIL at {' '.join(args)}: {last_err}")
    raise subprocess.CalledProcessError(1, list(args))


def read_info(path):
    values = {}
    with open(path, "r", encoding="ascii", errors="replace") as stream:
        for line in stream:
            if "=" in line:
                key, value = line.strip().split("=", 1)
                values[key] = value
    return values


def launch_agent(info_path, log_path, extra_fixture_args=()):
    global PIN_PROC, FIXTURE_PID
    FIXTURE_PID = None
    env = dict(**os.environ, PINBRIDGE_AGENT_PORT=PORT, PINBRIDGE_AGENT_LOG=log_path)
    PIN_PROC = subprocess.Popen(
        [PIN, "-t", AGENT, "--", FIXTURE, *extra_fixture_args,
         "--pinbridge-rpc-info", info_path],
        cwd=REL, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return PIN_PROC


def wait_ready(info_path, log_path, timeout_s=30.0):
    """Port answers ping AND the fixture info file is complete AND the
    embedded interpreter is up (kills the python-ready race for script ops)."""
    global FIXTURE_PID
    deadline = time.time() + timeout_s
    pid = None
    while time.time() < deadline:
        note_process_state()
        if pid is None:
            proc = run_cli("--port", PORT, "ping")
            if proc.returncode == 0:
                pid = json.loads(proc.stdout)["pid"]
        have_info = os.path.exists(info_path) and "exit_flag_address" in read_info(info_path)
        ready = False
        if os.path.exists(log_path):
            with open(log_path, "r", encoding="utf-8", errors="replace") as stream:
                ready = "python interpreter initialized" in stream.read()
        if pid is not None and have_info and ready:
            FIXTURE_PID = pid
            return read_info(info_path)
        time.sleep(0.25)
    note_process_state()
    raise AssertionError("agent never became ready (port/info/python)")


def script_run(path):
    """`script run` tolerant of the python-ready race and of the mailbox/tick
    collision on the single-threaded query server (see cli_script_op)."""
    deadline = time.time() + 45.0
    last_err = ""
    while True:
        note_process_state()
        proc = run_cli("--port", PORT, "script", "run", path)
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        last_err = (proc.stderr or "").strip()
        if "connect" in last_err.lower():
            note_process_state()  # raises AgentDead when the launcher is gone
        if time.time() >= deadline:
            break
        time.sleep(0.5)
    note_process_state()
    print(f"CLI_FAIL at script run {path}: {last_err}")
    raise subprocess.CalledProcessError(1, ["script", "run", path])


def script_output_text():
    reply = cli("script", "output")
    return "\n".join(entry["line"] for entry in reply["lines"])


def wait_output(wanted, absent=(), timeout_s=10.0):
    deadline = time.time() + timeout_s
    text = ""
    while time.time() < deadline:
        note_process_state()
        text = script_output_text()
        for bad in absent:
            if bad in text:
                raise AssertionError(f"failure marker {bad} in script output:\n{text}")
        if all(marker in text for marker in wanted):
            return text
        time.sleep(0.3)
    note_process_state()
    raise AssertionError(f"markers {wanted} never appeared in script output; tail:\n{text[-2000:]}")


def script_list():
    return cli("script", "list")["plugins"]


def wait_list(pred, what, timeout_s=10.0):
    deadline = time.time() + timeout_s
    entries = []
    while time.time() < deadline:
        note_process_state()
        entries = script_list()
        if pred(entries):
            return entries
        time.sleep(0.3)
    note_process_state()
    raise AssertionError(f"script list never satisfied: {what}; last: {entries}")


def stop_write_resume(address, hex_bytes):
    """stop -> write -> resume (the write op requires a stopped target)."""
    assert cli("stop")["stopped"], "stop failed"
    written = cli("write", hex(address), hex_bytes)
    assert written["written"] == len(hex_bytes) // 2, f"short write: {written}"
    assert cli("resume")["resumed"], "resume failed"


def clean_shutdown(info):
    """Asks the fixture to exit (exit flag) so Pin runs the agent's on_fini,
    then waits for the launcher. Returns the launcher exit code."""
    stop_write_resume(int(info["exit_flag_address"], 16), "01000000")
    try:
        return PIN_PROC.wait(timeout=15)
    except subprocess.TimeoutExpired:
        note_process_state()
        raise AssertionError("target did not exit within 15s of the exit flag")


def step(num, text):
    print(f"STEP {num}: {text}")


def check_benign_crash_dump():
    """After the handled-AV window: diag.rs's first-chance handler logs the
    fixture's deliberate AVs, which is expected. Every record must match the
    benign signature (AV, access address 0x1); a Pin-level crash record or
    any other code means a REAL crash and fails the test. Removes the file
    so step 12's absence check stays meaningful."""
    if not os.path.exists(CRASH_DUMP):
        return
    with open(CRASH_DUMP, "r", encoding="utf-8", errors="replace") as stream:
        dump = stream.read()
    assert "PIN_CRASH" not in dump, f"agent pin-level crash:\n{dump[:1500]}"
    for line in dump.splitlines():
        if line.startswith("CRASH"):
            assert "code=0xc0000005" in line and "access=0x1" in line, (
                f"unexpected crash record: {line}\n{dump[:1500]}")
    os.remove(CRASH_DUMP)


def attempt():
    global PIN_PROC
    # One session for the whole test: the fixture starts calm, step 9 turns
    # its handled-AV loop on/off through the control plane mid-run
    launch_agent(INFO, LOG)
    try:
        info = wait_ready(INFO, LOG)
        tick = int(info["tick_address"], 16)
        print(f"FIXTURE_OK tick={hex(tick)} pid={FIXTURE_PID}")

        step(1, "REJECT broken script")
        with open(BAD_PROBE, "w", encoding="ascii") as stream:
            stream.write("this is not python !!!")
        # retry only transport-level flakes (e.g. os error 10054/10060 in the
        # startup window); the compile rejection itself is the expected answer
        bad = None
        for _ in range(20):
            note_process_state()
            bad = run_cli("--port", PORT, "script", "run", BAD_PROBE)
            if bad.returncode == 0 or "server status 2" in (bad.stderr or ""):
                break
            time.sleep(0.5)
        assert bad.returncode != 0, "broken script was accepted"
        assert "server status 2" in bad.stderr, f"no server status 2: {bad.stderr}"
        assert "SyntaxError" in bad.stderr, f"no SyntaxError text: {bad.stderr}"
        print("  REJECT_OK server status 2 + SyntaxError surfaced")

        step(2, "LOAD_A resolve+bp+exact park+auto-resume probe")
        with open(PROBE_A_PATH, "w", encoding="ascii") as stream:
            stream.write(PROBE_A.replace("__TICK__", hex(tick)))
        loaded = script_run(PROBE_A_PATH)
        assert loaded["name"] == NAME_A, f"bad load reply: {loaded}"
        print(f"  LOAD_A_OK id={loaded['id']}")

        step(3, "LOAD_B exec watch on the main module")
        with open(PROBE_B_PATH, "w", encoding="ascii") as stream:
            stream.write(PROBE_B)
        loaded = script_run(PROBE_B_PATH)
        assert loaded["name"] == NAME_B, f"bad load reply: {loaded}"
        print(f"  LOAD_B_OK id={loaded['id']}")

        step(4, "CONCURRENT both plugins running, A_INIT+B_INIT in output")
        wait_list(
            lambda e: {p["name"] for p in e} == {NAME_A, NAME_B}
            and all(p["state"] == 1 for p in e),
            "both plugins running", 10)
        wait_output(["A_INIT", "B_INIT"], ["A_FAIL", "B_FAIL"], 10)
        print("  CONCURRENT_OK two plugins co-resident and initialized")

        step(5, "CALLBACKS A_HIT (bp+auto-resume) and B_BATCH (exec events)")
        wait_output(["A_HIT", "B_BATCH"], ["A_FAIL", "B_FAIL"], 10)
        counters = cli("counters")
        assert counters["exec"] > 0, f"exec counter stuck: {counters}"
        print(f"  CALLBACKS_OK exec={counters['exec']}")

        step(6, "EXPORTS rpc_tick matches the info file")
        exports = cli("exports", "pb_rpc_fixture.exe")["exports"]
        by_name = {entry["name"]: int(entry["address"], 16) for entry in exports}
        assert "rpc_tick" in by_name, f"rpc_tick missing: {by_name.keys()}"
        assert by_name["rpc_tick"] == tick, (
            f"exports rpc_tick {hex(by_name['rpc_tick'])} != info {hex(tick)}")
        print(f"  EXPORTS_OK rpc_tick={hex(tick)} count={len(exports)}")

        step(7, "HOOK set/list/counter/remove/clear")
        before = cli("counters")["hook_regs"]
        hooked = cli("hook", hex(tick))
        assert hooked["ok"], f"hook failed: {hooked}"
        hooks = [int(a, 16) for a in cli("hooks")["addresses"]]
        assert tick in hooks, f"hook not listed: {hooks}"
        time.sleep(2)
        after = cli("counters")["hook_regs"]
        assert after > before, f"hook_regs did not grow: {before} -> {after}"
        cli("hookdel", hex(tick))
        assert tick not in [int(a, 16) for a in cli("hooks")["addresses"]]
        cli("hookclear")
        assert cli("hooks")["count"] == 0, "hooks not empty after clear"
        print(f"  HOOK_OK hook_regs {before} -> {after}")

        step(8, "SYSCALL filter only/all around the syscall counter")
        cli("engine", ENGINE_SYSCALL, "on")  # on by default; make it explicit
        cli("syscallfilter", "only", "0xFFFF")
        time.sleep(0.5)
        s0 = cli("counters")["syscall"]
        time.sleep(2)
        s1 = cli("counters")["syscall"]
        assert s1 - s0 <= 1, f"filter only 0xFFFF leaked: {s0} -> {s1}"
        cli("syscallfilter", "all")  # restore
        time.sleep(0.5)
        s2 = cli("counters")["syscall"]
        time.sleep(2)
        s3 = cli("counters")["syscall"]
        assert s3 > s2, f"syscall counter stuck after filter all: {s2} -> {s3}"
        print(f"  SYSCALL_OK filtered={s1 - s0} unfiltered={s3 - s2}")

        step(9, "EXCEPTION on_exception(0xC0000005) sees a handled AV")
        # Quiet the flooding engines (memory/exec/branch) for this window —
        # the same discipline control_e2e uses for syscalls: plugin cursors
        # page the 64k ring oldest-first, so under the default ~1M events/s
        # flood a rare context_change event is evicted long before the
        # cursor reaches it (it only ever counts in the plugin's dropped).
        for kind in ("2", "3", "4"):
            cli("engine", kind, "off")
        with open(PROBE_EXC_PATH, "w", encoding="ascii") as stream:
            stream.write(PROBE_EXC)
        script_run(PROBE_EXC_PATH)
        # start the fixture's handled-AV loop (deref of 0x1 in __try/__except)
        stop_write_resume(int(info["raise_av_address"], 16), "01000000")
        wait_output(["EXC_SEEN 0xc0000005"], [], 15)
        print("  EXCEPTION_OK EXC_SEEN 0xc0000005 delivered to the probe")
        # stop the AVs, restore the engines (probe_b needs exec events again
        # for step 10), and retire the exception probe
        stop_write_resume(int(info["raise_av_address"], 16), "00000000")
        for kind in ("2", "3", "4"):
            cli("engine", kind, "on")
        cli_script_op("script", "off", NAME_EXC)
        # diag.rs logged the *handled* AVs to crash_dump.txt by design;
        # verify every record is that benign signature, then remove the file
        check_benign_crash_dump()

        step(10, "ISOLATION off probe_a leaves probe_b delivering")
        cli_script_op("script", "off", NAME_A)
        entries = wait_list(
            lambda e: [p["name"] for p in e] == [NAME_B],
            "only probe_b left", 5)
        delivered0 = entries[0]["delivered"]
        # the list snapshot republishes on a heartbeat (up to ~8s under ring
        # flood), so growth gets a wider window than the nominal 2s
        grew = False
        for _ in range(10):
            time.sleep(2)
            note_process_state()
            current = [p for p in script_list() if p["name"] == NAME_B]
            if current and current[0]["delivered"] > delivered0:
                grew = True
                break
        assert grew, f"probe_b delivered stuck at {delivered0}"
        wait_output(["A_UNLOAD"], [], 5)
        print(f"  ISOLATION_OK probe_b delivered {delivered0} -> "
              f"{current[0]['delivered']}, A_UNLOAD seen")

        step(11, "UNLOAD_ALL script off (no arg)")
        cli_script_op("script", "off")
        wait_list(lambda e: len(e) == 0, "list empty", 5)
        print("  UNLOAD_ALL_OK")

        step(12, "SHUTDOWN clean exit -> fini summary, no crash dump")
        assert not os.path.exists(CRASH_DUMP), (
            f"crash_dump.txt appeared during the main session:\n"
            f"{open(CRASH_DUMP, errors='replace').read()[:1500]}")
        code = clean_shutdown(info)
        with open(LOG, "r", encoding="utf-8", errors="replace") as stream:
            log_text = stream.read()
        assert "fini" in log_text and "total=" in log_text, (
            f"no fini summary in agent log; tail:\n{log_text[-1500:]}")
        assert not os.path.exists(CRASH_DUMP), (
            f"crash_dump.txt appeared at shutdown:\n"
            f"{open(CRASH_DUMP, errors='replace').read()[:1500]}")
        print(f"  SHUTDOWN_OK exit={code} fini summary logged")
    finally:
        if PIN_PROC is not None and PIN_PROC.poll() is None:
            PIN_PROC.kill()
        kill_pid(FIXTURE_PID)
    PIN_PROC = None


def main():
    cleanup_zombies()
    for stale in (INFO, LOG, CRASH_DUMP,
                  PROBE_A_PATH, PROBE_B_PATH, PROBE_EXC_PATH, BAD_PROBE):
        if os.path.exists(stale):
            os.remove(stale)
    for tries in (1, 2):
        try:
            attempt()
            print("SCRIPT_E2E_PASS")
            return 0
        except AgentDead as error:
            if tries == 2:
                raise
            print(f"AGENT_DIED ({error}); retrying the whole test from scratch")
            cleanup_zombies()
            for stale in (INFO, LOG, CRASH_DUMP):
                if os.path.exists(stale):
                    os.remove(stale)
            time.sleep(1)
    return 1


if __name__ == "__main__":
    sys.exit(main())
