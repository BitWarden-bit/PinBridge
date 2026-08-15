#!/usr/bin/env python3
"""recorder.py — live .pbtr recorder for PinBridge.

Talks the agent's binary query protocol DIRECTLY over loopback TCP (stdlib
socket + struct only). pinbridge-cli is deliberately NOT used: its JSON output
truncates event args.

Protocol (see bindings/rust/pinbridge-proto/src/lib.rs,
pinbridge-agent/src/query_server.rs):
    frame = [u32 payload_len LE incl op+status][u8 op][u8 status][payload]
    PING=0x01      empty -> [u32 maj][u32 min][u32 pid][u64 ring_total]
    COUNTERS=0x02  empty -> [u64 total][u64 dropped][u64 capacity][8 x u64 kinds]
    RING_PAGE=0x03 [u64 after][u64 limit<=2048]
                   -> [u64 total][u64 missed][u64 next][u64 count][count x 88B events]
    MODULES=0x17   empty -> [u32 count][count x (u64 low,u64 high,u8 is_main,
                              u32 nlen,name bytes)]
    ENGINE_SET=0x23 [u32 kind][u8 on] -> empty     (kind: 2=memory 3=exec 4=branch)

USAGE:
    python recorder.py --port 9011 --kinds exec,memory,branch --out win.pbtr \
        --seconds 3 [--main-module-only] [--target-name NAME]

LAUNCH PATTERN for the agent (engine instrumentation range is ENV-ONLY today):
    set PINBRIDGE_AGENT_PORT=9011
    set PINBRIDGE_AGENT_RANGE=0xLOW-0xHIGH   (e.g. main module range, keeps the
                                             event rate manageable)
    pin.exe -t pinbridge_agent.dll -- target.exe
Then run this recorder while the window of interest executes.

LOSSLESSNESS: the in-agent ring is finite; lossless capture holds only while
production < pull rate (~800K events/s observed over loopback). A ring overrun
shows up as missed>0 here and as sequence gaps in the .pbtr stats — such a
window is NOT valid for replay (re-record narrower). Fine for narrow windows.
"""

from __future__ import annotations

import argparse
import json
import socket
import struct
import sys
import time

import pbtrace

OP_PING = 0x01
OP_COUNTERS = 0x02
OP_RING_PAGE = 0x03
OP_MODULES = 0x17
OP_ENGINE_SET = 0x23

RING_PAGE_MAX_LIMIT = 2048

ENGINE_KINDS = {"memory": 2, "exec": 3, "branch": 4, "branch_edge": 4, "syscall": 5}


class ProtocolError(Exception):
    pass


class AgentClient:
    def __init__(self, host, port, timeout=10.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.sock.settimeout(timeout)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def _recv_exact(self, count):
        buf = b""
        while len(buf) < count:
            chunk = self.sock.recv(count - len(buf))
            if not chunk:
                raise ProtocolError("connection closed by agent")
            buf += chunk
        return buf

    def request(self, op, payload=b""):
        frame_len = len(payload) + 2
        if frame_len > (1 << 20):
            raise ProtocolError("frame too large")
        self.sock.sendall(struct.pack("<IBB", frame_len, op, 0) + payload)
        header = self._recv_exact(6)
        length, rop, status = struct.unpack("<IBB", header)
        if length < 2 or length > (1 << 20):
            raise ProtocolError("bad frame length %d" % length)
        body = self._recv_exact(length - 2)
        if rop != op:
            raise ProtocolError("op echo mismatch: sent 0x%02x got 0x%02x" % (op, rop))
        if status != 0:
            raise ProtocolError("op 0x%02x failed with status %d" % (op, status))
        return body

    # ---- typed helpers ----

    def ping(self):
        body = self.request(OP_PING)
        maj, min_, pid, total = struct.unpack_from("<IIIQ", body, 0)
        return {"abi_major": maj, "abi_minor": min_, "pid": pid, "ring_total": total}

    def counters(self):
        body = self.request(OP_COUNTERS)
        total, dropped, capacity = struct.unpack_from("<QQQ", body, 0)
        kinds = struct.unpack_from("<8Q", body, 24)
        return {"total": total, "dropped": dropped, "capacity": capacity,
                "kinds": list(kinds)}

    def ring_page(self, after, limit=RING_PAGE_MAX_LIMIT):
        body = self.request(OP_RING_PAGE, struct.pack("<QQ", after, limit))
        total, missed, next_, count = struct.unpack_from("<QQQQ", body, 0)
        events = []
        off = 32
        for _ in range(count):
            values = pbtrace.RECORD.unpack_from(body, off)
            off += pbtrace.RECORD_LEN
            events.append(pbtrace.Record(values[0], values[1], values[2],
                                         values[3], values[4:]))
        return total, missed, next_, events

    def modules(self):
        body = self.request(OP_MODULES)
        count = struct.unpack_from("<I", body, 0)[0]
        out = []
        off = 4
        for _ in range(count):
            low, high, is_main, nlen = struct.unpack_from("<QQBI", body, off)
            off += 21
            name = body[off:off + nlen].decode("utf-8", "replace")
            off += nlen
            out.append({"low": low, "high": high, "is_main": bool(is_main),
                        "name": name})
        return out

    def engine_set(self, kind, on):
        self.request(OP_ENGINE_SET, struct.pack("<IB", kind, 1 if on else 0))


def parse_kinds(text):
    kinds = []
    for token in text.split(","):
        token = token.strip().lower()
        if not token:
            continue
        if token not in ENGINE_KINDS:
            raise SystemExit("unknown engine kind %r (have: %s)"
                             % (token, ", ".join(sorted(ENGINE_KINDS))))
        kinds.append(ENGINE_KINDS[token])
    return sorted(set(kinds))


def main(argv):
    ap = argparse.ArgumentParser(description="record a live window to .pbtr")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=9001)
    ap.add_argument("--kinds", default="exec,memory",
                    help="comma list of engines: exec,memory,branch,syscall")
    ap.add_argument("--out", required=True)
    ap.add_argument("--seconds", type=float, default=3.0)
    ap.add_argument("--target-name", default=None,
                    help="meta target string (default: main module name)")
    ap.add_argument("--main-module-only", action="store_true",
                    help="keep only records whose ip is inside the main module")
    ap.add_argument("--settle-ms", type=int, default=200,
                    help="final drain wait after the engines are turned off")
    args = ap.parse_args(argv[1:])

    kind_ids = parse_kinds(args.kinds)
    client = AgentClient(args.host, args.port)
    recorded = 0
    missed_total = 0
    try:
        info = client.ping()
        print("[recorder] agent abi %d.%d pid=%d ring_total=%d"
              % (info["abi_major"], info["abi_minor"], info["pid"],
                 info["ring_total"]))
        modules = client.modules()
        main_mod = next((m for m in modules if m["is_main"]), None)
        target = args.target_name
        if target is None:
            target = main_mod["name"] if main_mod else "unknown"
        if main_mod:
            print("[recorder] main module %s [0x%x, 0x%x)"
                  % (main_mod["name"], main_mod["low"], main_mod["high"]))
        else:
            print("[recorder] WARNING: no main module reported")

        for kind in kind_ids:
            client.engine_set(kind, True)
        print("[recorder] engines on: %s — recording %.1fs ..."
              % (args.kinds, args.seconds))

        cursor = info["ring_total"]      # start at the live edge
        captured = []
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline:
            total, missed, cursor, events = client.ring_page(cursor)
            missed_total += missed
            if events:
                captured.extend(events)
            else:
                time.sleep(0.0005)       # busy ring or no new events

        for kind in kind_ids:
            client.engine_set(kind, False)

        # final drain: let the agent submit stragglers, then page until caught up
        time.sleep(max(args.settle_ms, 0) / 1000.0)
        idle_rounds = 0
        while idle_rounds < 3:
            total, missed, cursor, events = client.ring_page(cursor)
            missed_total += missed
            if events:
                captured.extend(events)
                idle_rounds = 0
            else:
                idle_rounds += 1
                time.sleep(0.005)
        print("[recorder] engines off; pulled %d events, ring missed %d"
              % (len(captured), missed_total))

        # local gap cross-check (ring overrun shows as sequence holes)
        local_gaps = 0
        prev = None
        for rec in captured:
            if prev is not None and rec.sequence > prev + 1:
                local_gaps += rec.sequence - prev - 1
            prev = rec.sequence
        if local_gaps:
            print("[recorder] WARNING: %d sequence holes in captured stream "
                  "(window NOT lossless — re-record narrower)" % local_gaps)

        if args.main_module_only and main_mod:
            low, high = main_mod["low"], main_mod["high"]
            before = len(captured)
            captured = [r for r in captured if low <= r.address < high]
            print("[recorder] --main-module-only: kept %d/%d records"
                  % (len(captured), before))
            post_filtered = True
        else:
            post_filtered = False

        meta = {
            "target": target,
            "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "kinds": sorted({r.kind for r in captured}),
            "requested_kinds": kind_ids,
            "agent": {"abi_major": info["abi_major"],
                      "abi_minor": info["abi_minor"], "pid": info["pid"]},
            "main_module": main_mod,
            "duration_s": args.seconds,
            "ring_missed": missed_total,
            "post_filtered": post_filtered,
            "format": {"version": 1, "repeat_kind": pbtrace.KIND_REPEAT,
                       "repeat_encoding": "rle"},
        }
        with pbtrace.TraceWriter(args.out, meta, compress_repeats=True) as writer:
            # window markers (kind 11): tag 1 = start, 2 = end
            if captured:
                writer.emit(captured[0].sequence, pbtrace.KIND_MARKER, 0, 0,
                            1, int(time.time()))
            for rec in captured:
                writer.emit_record(rec)
            if captured:
                writer.emit(captured[-1].sequence + 1, pbtrace.KIND_MARKER, 0, 0,
                            2, missed_total)
            recorded = writer.count
        print("[recorder] wrote %d logical / %d physical records to %s "
              "(%.2fx, missed=%d%s)"
              % (recorded, writer.physical_count, args.out,
                 (float(recorded) / writer.physical_count
                  if writer.physical_count else 1.0), missed_total,
                 "" if missed_total == 0 and local_gaps == 0
                 else " — LOSSY WINDOW, do not replay"))
        return 0 if missed_total == 0 and local_gaps == 0 else 1
    finally:
        client.close()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
