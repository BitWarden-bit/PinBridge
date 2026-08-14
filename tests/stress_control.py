# Stress the control plane: rapid stop/resume/step cycles. If the agent
# wedges (deadlock), a CLI call starts timing out -> repro for "卡死".
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
VMP = None
vt = os.path.join(REPO, "build", "vmp_target.txt")
if os.path.exists(vt):
    VMP = open(vt, encoding="utf-8").read().strip()
PIN = os.environ.get("PINBRIDGE_PIN_EXE") or (
    os.environ.get("PIN_ROOT") + r"\intel64\bin\pin.exe" if os.environ.get("PIN_ROOT") else None)
if not PIN:
    raise SystemExit(
        "pin.exe not found: set PINBRIDGE_PIN_EXE to the full path of pin.exe "
        "(or PIN_ROOT to your Pin 3.31 SDK root)")
INFO = REL + r"\rpc_fixture_info_stress.txt"
PORT = "9011"


def kill_trees():
    subprocess.run(
        ["powershell", "-NoProfile", "-Command",
         "Get-Process pb_rpc_fixture,'crypto.vmp',pin -ErrorAction SilentlyContinue | Stop-Process -Force"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cli(*args, timeout=15):
    proc = subprocess.run([CLI, "--port", PORT, *args],
                          capture_output=True, timeout=timeout)
    if proc.returncode != 0:
        raise RuntimeError(f"cli {' '.join(args)} -> {(proc.stderr or b'').decode('utf-8', 'replace').strip()}")
    return json.loads(proc.stdout.decode("utf-8", "replace"))


def stress(target, cycles, tag):
    env = dict(**os.environ, PINBRIDGE_AGENT_PORT=PORT)
    pin = subprocess.Popen([PIN, "-t", AGENT, "--", target],
                           cwd=REL, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(60):
            try:
                cli("ping", timeout=2)
                break
            except Exception:
                time.sleep(0.3)
        t0 = time.time()
        for i in range(cycles):
            cli("stop")
            ids = cli("threads")["thread_ids"]
            if i % 3 == 0 and ids:
                cli("si", str(ids[0]))
            elif i % 3 == 1 and ids:
                cli("so", str(ids[0]))
            cli("resume")
            if i % 10 == 9:
                print(f"  {tag} cycle {i + 1}/{cycles} ok ({time.time() - t0:.1f}s)", flush=True)
        print(f"STRESS_{tag}_PASS cycles={cycles} in {time.time() - t0:.1f}s", flush=True)
        return True
    except Exception as error:
        print(f"STRESS_{tag}_WEDGED: {error}", flush=True)
        return False
    finally:
        kill_trees()
        pin.kill()


def main():
    ok = stress(FIXTURE, 200, "FIXTURE")
    if VMP:
        ok = stress(VMP, 30, "VMP") and ok
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
