# Scratch diagnostic: why do breakpoints and stepping fail on crypto.vmp.exe?
# Walks the debugger chain end to end and prints where it breaks.
import json
import os
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REL = os.path.join(REPO, "bindings", "rust", "target", "release")
CLI = REL + r"\pinbridge-cli.exe"
AGENT = REL + r"\pinbridge_agent.dll"
# VMP-protected sample to walk: env override first, then the repo-local
# build\vmp_target.txt pointer file (same convention as stress_control.py).
TARGET = os.environ.get("PINBRIDGE_VMP_TARGET")
if not TARGET:
    vt = os.path.join(REPO, "build", "vmp_target.txt")
    if os.path.exists(vt):
        TARGET = open(vt, encoding="utf-8").read().strip()
if not TARGET:
    raise SystemExit(
        "no VMP target: set PINBRIDGE_VMP_TARGET to the VMP-protected sample "
        "executable (or write its path to build\\vmp_target.txt)")
PIN = os.environ.get("PINBRIDGE_PIN_EXE") or (
    os.environ.get("PIN_ROOT") + r"\intel64\bin\pin.exe" if os.environ.get("PIN_ROOT") else None)
if not PIN:
    raise SystemExit(
        "pin.exe not found: set PINBRIDGE_PIN_EXE to the full path of pin.exe "
        "(or PIN_ROOT to your Pin 3.31 SDK root)")
PORT = "9241"


def kill_target_tree():
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command",
         "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'crypto.vmp.exe' }"
         " | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cli(*args, retries=3):
    full = ["--port", PORT, *args]
    for attempt in range(retries):
        proc = subprocess.run([CLI, *full], capture_output=True, timeout=30)
        if proc.returncode == 0:
            return json.loads(proc.stdout.decode("utf-8", "replace"))
        err = (proc.stderr or b"").decode("utf-8", "replace").strip()
        if attempt == retries - 1:
            print(f"CLI_FAIL at {' '.join(args)}: {err}")
            return None
        time.sleep(0.5)


def main():
    kill_target_tree()
    env = dict(**os.environ, PINBRIDGE_AGENT_PORT=PORT)
    pin = subprocess.Popen(
        [PIN, "-t", AGENT, "--", TARGET],
        cwd=REL, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        # 1) wait for the query server
        ping = None
        for _ in range(60):
            ping = cli("ping", retries=1)
            if ping:
                break
            time.sleep(0.5)
        print(f"1 PING: {ping}")
        if not ping:
            print("BREAK: agent query server never came up")
            return

        # 2) is the target actually executing?
        c0 = cli("counters")
        time.sleep(2)
        c1 = cli("counters")
        rate = (c1["total"] - c0["total"]) / 2 if c0 and c1 else -1
        alive = pin.poll() is None
        print(f"2 COUNTERS: total {c0['total']} -> {c1['total']} (~{rate:.0f}/s) pin_alive={alive}")

        # 3) modules
        mods = cli("modules")
        if mods:
            for m in mods["modules"]:
                mark = "MAIN" if m["main"] else ""
                print(f"3 MODULE {mark} {hex(m['low'])}-{hex(m['high'])} {m['name'].split(chr(92))[-1]}")

        # 4) stop -> threads -> context
        t0 = time.time()
        stop = cli("stop")
        print(f"4 STOP: {stop} ({time.time()-t0:.2f}s)")
        threads = cli("threads")
        print(f"5 THREADS: {threads}")
        if not threads or not threads.get("thread_ids"):
            print("BREAK: no stopped threads -> nothing to step/bp")
            return
        tid = threads["thread_ids"][0]
        ctx = cli("context", str(tid))
        regs = ctx["registers"] if ctx else {}
        rip = int(regs.get("rip", "0x0"), 16)
        rsp = int(regs.get("rsp", "0x0"), 16)
        print(f"6 CONTEXT tid={tid} rip={hex(rip)} rsp={hex(rsp)}")
        in_main = mods and any(m["main"] and m["low"] <= rip < m["high"] for m in mods["modules"])
        print(f"  rip in main module: {in_main}")

        # 7) disasm at rip
        dis = cli("disasm", hex(rip), "5")
        if dis:
            for r in dis["insns"]:
                print(f"7 DIS {r['address']}: {r['text']}")

        # 8) breakpoint exactly at the stopped rip -> resume -> must hit
        bp = cli("bp", hex(rip))
        print(f"8 BP_SET at rip: {bp}")
        cli("resume")
        hit = None
        deadline = time.time() + 8
        while time.time() < deadline:
            bps = cli("bps")
            if bps and bps["stopped"] and bps["breakpoints"] and bps["breakpoints"][0]["hits"] > 0:
                hit = bps
                break
            time.sleep(0.1)
        print(f"9 BP_AT_RIP: {'HIT stopped hit_tid=' + str(hit.get('hit_tid')) if hit else 'MISS within 8s'}")

        # 9) step into from here
        if hit:
            before = rip
            st = cli("si", str(tid))
            time.sleep(0.3)
            ctx2 = cli("context", str(tid))
            rip2 = int(ctx2["registers"].get("rip", "0x0"), 16) if ctx2 else 0
            print(f"10 STEP_INTO: {st} rip {hex(before)} -> {hex(rip2)} {'OK' if rip2 and rip2 != before else 'NO-ADVANCE'}")
    finally:
        kill_target_tree()
        pin.kill()


if __name__ == "__main__":
    main()
