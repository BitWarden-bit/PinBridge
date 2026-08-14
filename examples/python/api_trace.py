# api_trace.py: break on an exported API by name and dump its arguments
# (Win64: rcx rdx r8 r9) every time it fires, then auto-resume.
#
# Load:  pinbridge-cli --port 9012 script run api_trace.py
# Edit TARGET to whatever module!export you care about.
#
# pb API v2: bp hits arrive as on_bp_hit(evt {tid, addr, id}) while the
# target is held stopped; pb.resume() lets it continue. pb.print lines land
# in the agent's output ring — read them with `script output [--follow]`.

import pb

TARGET = "kernel32.dll!VirtualProtect"
MAX_HITS = 20
hits = 0
bp_id = None

addr = pb.resolve_name(TARGET)
if addr is None:
    pb.print("api_trace: cannot resolve " + TARGET)
else:
    pb.print("api_trace: %s at 0x%x" % (TARGET, addr))
    bp_id = pb.bp_set(addr)
    pb.print("api_trace: breakpoint id=%s" % bp_id)

def on_bp_hit(evt):
    global hits
    tid = evt["tid"]
    if tid < 0:
        return  # manual pause, not our breakpoint
    hits += 1
    rcx = pb.get_reg(tid, "rcx") or 0
    rdx = pb.get_reg(tid, "rdx") or 0
    r8 = pb.get_reg(tid, "r8") or 0
    r9 = pb.get_reg(tid, "r9") or 0
    pb.print("api_trace hit #%d %s(rcx=0x%x, rdx=0x%x, r8=0x%x, r9=0x%x)"
             % (hits, TARGET, rcx, rdx, r8, r9))
    if hits >= MAX_HITS:
        if bp_id is not None:
            pb.bp_remove(bp_id)
        pb.print("api_trace: done, breakpoint removed")
        return  # stay stopped for the human/operator
    pb.resume()

def on_unload():
    if bp_id is not None:
        try:
            pb.bp_remove(bp_id)
        except Exception:
            pass
    pb.print("api_trace: unloaded")
