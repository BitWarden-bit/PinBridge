# unpack_guard.py: SKELETON for the exception-takeover unpacking pattern.
#
# VMP-style protectors route their own exceptions through SEH. The classic
# unpacking move is to intercept KiUserExceptionDispatcher (KUED): every
# exception that reaches user mode funnels through it BEFORE the app's SEH
# chain runs, so a breakpoint there is a take-over decision point: handle
# the exception yourself (dump/fix up/redirect) or let it through.
#
# Two channels demonstrated side by side:
#   OBSERVE  — on_exception: agent notifies us when an exception is seen.
#   TAKEOVER — bp on KUED: target is held stopped, we decide what happens.
#
# Load:  pinbridge-cli --port 9012 script run unpack_guard.py
# Read:  pinbridge-cli --port 9012 script output --follow

import struct
import pb

MAX_HITS = 100          # safeguard: give up (remove bp) after this many
hits = 0
bp_id = None

pb.on_exception(codes=[0xC0000005])   # observer channel (STATUS_ACCESS_VIOLATION)

def on_exception(evt):
    # evt: {tid, code, rip, reason} — pure observation, no control flow.
    pb.print("unpack_guard OBSERVE: exception 0x%08x at rip=0x%x tid=%d"
             % (evt["code"], evt["rip"], evt["tid"]))

def pb_init():
    global bp_id
    addr = pb.resolve_name("ntdll.dll!KiUserExceptionDispatcher")
    if addr is None:
        pb.print("unpack_guard: cannot resolve KiUserExceptionDispatcher")
        return
    bp_id = pb.bp_set(addr)
    pb.print("unpack_guard: KUED bp id=%s at 0x%x" % (bp_id, addr))

# --- decision hook: YOU implement this --------------------------------------
# Return True to take the exception over (default False = pass-through).
# Typical recognition: faulting address inside the just-unpacked region,
# exception count crossing the protector's known threshold, etc.
def should_take_over(exc_code, exc_addr, tid):
    return False

# --- example dump routine ----------------------------------------------------
# Full CPython file IO works inside the agent. read_mem is capped at 1MB
# per call, so chunk. Returns bytes written, or None on read failure.
def dump_region(addr, size, path):
    CHUNK = 1 << 20
    with open(path, "wb") as f:
        written = 0
        while written < size:
            n = min(CHUNK, size - written)
            data = pb.read_mem(addr + written, n)
            if data is None:
                pb.print("unpack_guard: read_mem failed at 0x%x"
                         % (addr + written))
                return None
            f.write(bytes(data))
            written += len(data)
    pb.print("unpack_guard: dumped 0x%x bytes from 0x%x -> %s"
             % (size, addr, path))
    return size

def on_bp_hit(evt):
    global hits, bp_id
    tid = evt["tid"]
    if tid < 0 or evt["addr"] == 0:
        pb.resume()
        return
    hits += 1
    if hits > MAX_HITS:
        pb.print("unpack_guard: MAX_HITS reached, removing bp")
        if bp_id is not None:
            pb.bp_remove(bp_id)
            bp_id = None
        pb.resume()
        return

    # --- parse the dispatcher frame (NEEDS-PER-TARGET-VERIFICATION!) --------
    # x64 Windows, KUED entry: rcx/rdx are NOT reliable here (the kernel
    # switched context). The classic layout puts the EXCEPTION_RECORD*
    # at [rsp] and the CONTEXT* at [rsp+8] — this has held on many builds
    # but is NOT ABI-guaranteed, hence the defensive sanity check below:
    # a plausible NTSTATUS has a severity top nibble of 0x8/0xC/0xE.
    rsp = pb.get_reg(tid, "rsp") or 0
    frame = pb.read_mem(rsp, 0x100)
    if frame is None:
        pb.print("unpack_guard: frame read failed at rsp=0x%x" % rsp)
        pb.resume()
        return
    er_ptr, _ctx_ptr = struct.unpack_from("<QQ", bytes(frame), 0)
    er = pb.read_mem(er_ptr, 0x10)
    if er is None:
        pb.print("unpack_guard: EXCEPTION_RECORD unreadable at 0x%x" % er_ptr)
        pb.resume()
        return
    code, _flags, _chain, exc_addr = struct.unpack_from("<IIQQ", bytes(er), 0)
    if (code >> 28) not in (0x8, 0xC, 0xE):
        pb.print("unpack_guard: ER parse looks wrong (code=0x%08x) — "
                 "verify frame layout for this build" % code)
        pb.resume()
        return

    pb.print("unpack_guard KUED #%d: code=0x%08x addr=0x%x tid=%d"
             % (hits, code, exc_addr, tid))

    if should_take_over(code, exc_addr, tid):
        # Example take-over: dump 4KB around the faulting address, then let
        # the app continue. Real scripts also skip/redirect, e.g.:
        #   dump_region(exc_addr & ~0xFFF, 0x1000, "dump_hit%d.bin" % hits)
        #   pb.set_reg(tid, "rip", <patched target>)   # redirect execution
        # Adjust to your dump-timing recognition logic.
        pb.print("unpack_guard: TAKEOVER (stub) at 0x%x" % exc_addr)

    pb.resume()  # default path: deliver to the app's SEH chain

def on_unload():
    if bp_id is not None:
        try:
            pb.bp_remove(bp_id)
        except Exception:
            pass
    pb.print("unpack_guard: unloaded (%d KUED hits)" % hits)
