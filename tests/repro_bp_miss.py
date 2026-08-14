# Scratch: hunt the reported breakpoint "miss". Exercises the GUI-like flow:
# 1) bp -> hit -> resume -> re-hit, for several cycles (continue repeatedly)
# 2) while stopped, plant a second bp a few instructions ahead, resume,
#    expect it to hit.
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
INFO = REL + r"\rpc_fixture_info_miss.txt"
PORT = "9238"


def kill_target_tree():
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
         "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'pb_rpc_fixture.exe' }"
         " | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cli(*args, retries=8):
    full = ["--port", PORT, *args]
    for attempt in range(retries):
        proc = subprocess.run([CLI, *full], capture_output=True)
        if proc.returncode == 0:
            return json.loads(proc.stdout.decode("utf-8", "replace"))
        if attempt == retries - 1:
            print(f"CLI_FAIL at {' '.join(args)}: {(proc.stderr or b'').decode('utf-8','replace').strip()}")
            raise subprocess.CalledProcessError(proc.returncode, full)
        time.sleep(0.3)


def wait_stopped(timeout=5.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        bps = cli("bps")
        if bps["stopped"]:
            return bps
        time.sleep(0.05)
    return None


def resume_and_expect_rehit(cycle, expect_hits):
    cli("resume")
    # must observe a running window (replay suppression working)
    running = 0
    deadline = time.time() + 3.0
    stopped = None
    while time.time() < deadline:
        bps = cli("bps")
        if bps["stopped"]:
            stopped = bps
            if running:
                break
        else:
            running += 1
        time.sleep(0.005)
    if not stopped:
        print(f"cycle {cycle}: MISS - never stopped again after resume")
        return False
    hits = stopped["breakpoints"][0]["hits"] if stopped["breakpoints"] else -1
    if not running:
        print(f"cycle {cycle}: REPLAY - stopped instantly, no running window")
        return False
    if hits != expect_hits:
        print(f"cycle {cycle}: HITS drift: got {hits}, want {expect_hits}")
    print(f"cycle {cycle}: ok running_polls={running} hits={hits}")
    return True


def main():
    kill_target_tree()
    if os.path.exists(INFO):
        os.remove(INFO)
    env = dict(**os.environ, PINBRIDGE_AGENT_PORT=PORT)
    pin = subprocess.Popen(
        [PIN, "-t", AGENT, "--", FIXTURE, "--pinbridge-rpc-info", INFO],
        cwd=REL, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    fails = 0
    try:
        info = {}
        for _ in range(50):
            if os.path.exists(INFO):
                with open(INFO, "r", encoding="ascii", errors="replace") as stream:
                    for line in stream:
                        if "=" in line:
                            k, v = line.strip().split("=", 1)
                            info[k] = v
                if "tick_address" in info:
                    break
            time.sleep(0.2)
        tick = int(info["tick_address"], 16)

        bp = cli("bp", hex(tick))
        first = wait_stopped()
        assert first, "initial hit never happened"
        print(f"initial hit ok hits={first['breakpoints'][0]['hits']} hit_tid={first.get('hit_tid')}")

        # 1) repeated continue cycles
        for cycle in range(1, 5):
            if not resume_and_expect_rehit(cycle, cycle + 1):
                fails += 1

        # 2) while stopped, plant a second bp a few instructions into tick
        rows = cli("disasm", hex(tick), "8")["insns"]
        second_addr = int(rows[2]["address"], 16)
        cli("bp", hex(second_addr))
        cli("resume")
        seen = None
        deadline = time.time() + 5.0
        while time.time() < deadline:
            bps = cli("bps")
            if bps["stopped"] and len(bps["breakpoints"]) >= 2 and bps["breakpoints"][1]["hits"] > 0:
                seen = bps
                break
            if bps["stopped"] and bps.get("hit_addr", "0x0").lower() == hex(second_addr):
                seen = bps
                break
            if not bps["stopped"]:
                time.sleep(0.01)
            else:
                # stopped at the first bp again; keep going
                cli("resume")
        if seen:
            print(f"second bp while stopped: OK hit_addr={seen.get('hit_addr')}")
        else:
            print("second bp while stopped: MISS within 5s")
            fails += 1
    finally:
        kill_target_tree()
        pin.kill()
    print("MISS_HUNT_" + ("FAIL" if fails else "PASS"))
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
