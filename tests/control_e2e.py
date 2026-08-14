# Control-plane E2E: stop/resume/read/write/breakpoint against the rpc_fixture
# busy-tick target. The fixture prints tick_address to an info file at startup.
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
INFO = REL + r"\rpc_fixture_info.txt"
PORT = "9011"


def kill_target_tree():
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
         "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'pb_rpc_fixture.exe' }"
         " | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cli(*args, retries=5):
    full = ["--port", PORT, *args]
    for attempt in range(retries):
        proc = subprocess.run([CLI, *full], capture_output=True, text=True)
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        if attempt == retries - 1:
            print(f"CLI_FAIL at {' '.join(args)}: {(proc.stderr or '').strip()}")
            raise subprocess.CalledProcessError(proc.returncode, full)
        time.sleep(0.3)


def read_info():
    values = {}
    with open(INFO, "r", encoding="ascii", errors="replace") as stream:
        for line in stream:
            if "=" in line:
                key, value = line.strip().split("=", 1)
                values[key] = value
    return values


def main():
    kill_target_tree()
    if os.path.exists(INFO):
        os.remove(INFO)
    env = dict(**os.environ, PINBRIDGE_AGENT_PORT=PORT)
    pin = subprocess.Popen(
        [PIN, "-t", AGENT, "--", FIXTURE, "--pinbridge-rpc-info", INFO],
        cwd=REL, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        info = {}
        for _ in range(50):
            if os.path.exists(INFO):
                info = read_info()
                if "tick_address" in info and "memory_hex" in info:
                    break
            time.sleep(0.2)
        tick = int(info["tick_address"], 16)
        memory = int(info["memory_address"], 16)
        memory_hex = info["memory_hex"]
        print(f"FIXTURE_OK tick={hex(tick)} memory={hex(memory)}")

        modules = cli("modules")
        mains = [m for m in modules["modules"] if m["main"]]
        assert mains and mains[0]["low"] <= tick < mains[0]["high"], "tick not in main module"
        print(f"MODULES_OK count={modules['count']}")

        bp = cli("bp", hex(tick))
        bp_id = bp["id"]
        print(f"BP_SET_OK id={bp_id}")

        hit = None
        for _ in range(100):
            bps = cli("bps")
            if bps["breakpoints"] and bps["breakpoints"][0]["hits"] > 0 and bps["stopped"]:
                hit = bps
                break
            time.sleep(0.2)
        assert hit, "breakpoint never hit/stopped"
        print(f"BP_HIT_OK hits={hit['breakpoints'][0]['hits']} stopped={hit['stopped']}")

        # exact stop: the hit thread's saved rip IS the breakpoint address
        # (redirect-to-park rewinds the context; no instruction past the bp
        # has executed)
        hit_tid = hit.get("hit_tid", 0xFFFFFFFF)
        assert hit_tid != 0xFFFFFFFF, "no hit thread recorded"
        ctx = cli("context", str(hit_tid))
        stop_rip = int(ctx["registers"]["rip"], 16)
        assert stop_rip == tick, f"stop rip {hex(stop_rip)} != bp {hex(tick)}"
        print(f"STOP_EXACT_OK rip={hex(stop_rip)}")

        read = cli("read", hex(memory), "16")
        assert read["data"] == memory_hex, f"read {read['data']} != fixture {memory_hex}"
        print(f"READ_OK data={read['data']}")

        write = cli("write", hex(memory), "ff" * 16)
        assert write["written"] == 16, f"short write: {write['written']}"
        reread = cli("read", hex(memory), "16")
        assert reread["data"] == "ff" * 16, "readback mismatch"
        restore = cli("write", hex(memory), memory_hex)
        assert restore["written"] == 16
        print("WRITE_OK readback verified and restored")

        # Resume over the breakpoint: the replayed execution of the bp
        # instruction must be swallowed (instrumentation bps have no 0xCC to
        # restore), so the app runs until the next real loop hit (~100ms).
        # A replay bug re-stops in the same millisecond as the resume.
        cli("resume")
        running_seen = 0
        stopped_again = False
        for _ in range(60):
            bps = cli("bps")
            if bps["stopped"]:
                stopped_again = True
                if running_seen:
                    break
            else:
                running_seen += 1
            time.sleep(0.005)
        assert running_seen >= 1 and stopped_again, (
            f"resume re-stopped on the replayed bp (running={running_seen} stopped={stopped_again})")
        print(f"RESUME_OVER_BP_OK running_polls={running_seen}")

        removed = cli("bc", str(bp_id))
        assert removed["removed"] == bp_id
        print("BP_REMOVE_OK")

        resumed = cli("resume")
        assert resumed["resumed"], "resume failed"
        time.sleep(0.5)
        after = cli("bps")
        assert not after["stopped"], "still stopped after resume"
        print("RESUME_OK running again")

        stopped = cli("stop")
        assert stopped["stopped"], "stop failed"
        again = cli("bps")
        assert again["stopped"], "stop did not take effect"

        # single-step with the breakpoint gone: si lands on a strictly new
        # address; second si too; so lands and stops again (degenerates to si
        # on non-call instructions).
        tid = cli("threads")["thread_ids"][0]
        rip0 = int(cli("context", str(tid))["registers"]["rip"], 16)
        assert rip0 != 0
        assert cli("si", str(tid))["ok"], "si failed"
        for _ in range(50):
            if cli("bps")["stopped"]:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("si never stopped")
        rip1 = int(cli("context", str(tid))["registers"]["rip"], 16)
        assert rip1 != rip0, "si did not advance rip"
        assert cli("si", str(tid))["ok"], "second si failed"
        for _ in range(50):
            if cli("bps")["stopped"]:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("second si never stopped")
        rip2 = int(cli("context", str(tid))["registers"]["rip"], 16)
        assert rip2 != rip1, "second si did not advance rip"
        assert cli("so", str(tid))["ok"], "so failed"
        for _ in range(50):
            if cli("bps")["stopped"]:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("so never stopped")
        rip3 = int(cli("context", str(tid))["registers"]["rip"], 16)
        print(f"STEP_OK rip {hex(rip0)} -> si {hex(rip1)} -> si {hex(rip2)} -> so {hex(rip3)}")

        resumed2 = cli("resume")
        assert resumed2["resumed"], "second resume failed"
        print("STOP_RESUME_OK")

        # syscall engine: silence the noisy engines so syscalls fill the ring
        for kind in ("2", "3", "4"):
            cli("engine", kind, "off")
        time.sleep(1.0)
        page = cli("events", "512")
        syscalls = [e for e in page["events"] if e["kind"] == 5]
        entries = [e for e in syscalls if e["arg1"] == 0]
        exits = [e for e in syscalls if e["arg1"] == 1]
        assert entries and exits, f"syscall events missing: {len(syscalls)}"
        print(f"SYSCALL_OK entries={len(entries)} exits={len(exits)} number={entries[0]['arg0']}")

        # thread context read/write
        cli("stop")
        threads = cli("threads")
        assert threads["count"] > 0, "no stopped threads"
        tid = threads["thread_ids"][0]
        context = cli("context", str(tid))
        rip = int(context["registers"]["rip"], 16)
        assert rip != 0, "rip is zero"
        old_r8 = int(context["registers"]["r8"], 16)
        cli("setreg", str(tid), "r8", "0x1234")
        after_set = cli("context", str(tid))
        assert int(after_set["registers"]["r8"], 16) == 0x1234, "setreg did not take"
        cli("setreg", str(tid), "r8", hex(old_r8))
        restored = cli("context", str(tid))
        assert int(restored["registers"]["r8"], 16) == old_r8, "restore failed"
        print(f"CONTEXT_OK tid={tid} rip={hex(rip)} setreg write/readback/restore")
        cli("resume")

        # exception pause policy (set/get only; no live exception here)
        cli("exc", "all")
        policy = cli("exc")
        assert policy["enabled"] and policy["exception_code"] == 0
        cli("exc", "off")
        policy = cli("exc")
        assert not policy["enabled"]
        print("EXC_POLICY_OK")
        print("CONTROL_E2E_PASS")
        return 0
    finally:
        pin.kill()
        kill_target_tree()


if __name__ == "__main__":
    sys.exit(main())
