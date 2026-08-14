#!/usr/bin/env python3
"""pbtrace.py — reader/writer for PinBridge offline trace recordings (.pbtr).

File format (fixed contract, all integers little-endian):

    0:  "PBTR" (4 bytes)
    4:  u32 version (= 1)
    8:  u32 meta_len
    12: u32 reserved (= 0)
    16: meta_len bytes UTF-8 JSON: {"target": str, "created": str, "kinds": [u32], ...}
    then fixed 88-byte records:
      +0  u64 sequence
      +8  u32 kind
      +12 u32 thread_id
      +16 u64 address
      +24 u64 arg0 .. +80 u64 arg7

Kind layouts:
    kind 3  exec:        address=ip, arg0=static instruction length
    kind 2  memory:      address=ip, arg0=ea, arg1=size, arg2=access (0=read,1=write,2=read2)
    kind 4  branch_edge: address=ip, arg0=target, arg1=taken
    kind 9  exec_bytes:  address=ip, arg0=static_len, arg1=bytes[0..8), arg2=bytes[8..15) zero-padded
    kind 10 mem_value:   address=ip, arg0=ea, arg1=size, arg2=access, arg3=value LE zero-padded
    kind 11 marker:      address=0, arg0=tag, arg1=value

Readers MUST skip unknown kinds and tolerate a truncated tail record.
Multi-thread: records interleave; split by thread_id for single-thread windows.

Pure stdlib (struct + json).
"""

from __future__ import annotations

import json
import struct
import sys

MAGIC = b"PBTR"
VERSION = 1
HEADER = struct.Struct("<4sIII")          # magic, version, meta_len, reserved
HEADER_LEN = HEADER.size                   # 16
RECORD = struct.Struct("<QIIQ8Q")          # seq, kind, tid, address, arg0..arg7
RECORD_LEN = RECORD.size                   # 88

# kind ids (1..8 mirror the agent's EVENT_* ids; 9..11 are recording-only)
KIND_HOOK_REGS = 1
KIND_MEMORY = 2
KIND_EXEC = 3
KIND_BRANCH = 4
KIND_SYSCALL = 5
KIND_CONTEXT_CHANGE = 6
KIND_MODULE_LOAD = 7
KIND_MODULE_UNLOAD = 8
KIND_EXEC_BYTES = 9
KIND_MEM_VALUE = 10
KIND_MARKER = 11

KIND_NAMES = {
    KIND_HOOK_REGS: "hook_regs",
    KIND_MEMORY: "memory",
    KIND_EXEC: "exec",
    KIND_BRANCH: "branch_edge",
    KIND_SYSCALL: "syscall",
    KIND_CONTEXT_CHANGE: "context_change",
    KIND_MODULE_LOAD: "module_load",
    KIND_MODULE_UNLOAD: "module_unload",
    KIND_EXEC_BYTES: "exec_bytes",
    KIND_MEM_VALUE: "mem_value",
    KIND_MARKER: "marker",
}
NAME_KINDS = {name: kind for kind, name in KIND_NAMES.items()}

ACCESS_READ = 0
ACCESS_WRITE = 1
ACCESS_READ2 = 2


class Record:
    """One 88-byte trace record."""
    __slots__ = ("sequence", "kind", "thread_id", "address", "args")

    def __init__(self, sequence, kind, thread_id, address, args):
        self.sequence = sequence
        self.kind = kind
        self.thread_id = thread_id
        self.address = address
        self.args = tuple(args)            # always 8 u64 slots

    @property
    def arg0(self): return self.args[0]

    @property
    def arg1(self): return self.args[1]

    @property
    def arg2(self): return self.args[2]

    @property
    def arg3(self): return self.args[3]

    @property
    def kind_name(self):
        return KIND_NAMES.get(self.kind, "unknown_%d" % self.kind)

    def exec_bytes(self):
        """kind-9 payload -> raw instruction bytes (arg0 = static length)."""
        length = self.arg0 & 0xFF
        raw = struct.pack("<QQ", self.arg1, self.arg2)
        return raw[:min(length, 15)]

    def __repr__(self):
        return ("Record(seq=%d %s tid=%d addr=0x%x args=%s)"
                % (self.sequence, self.kind_name, self.thread_id,
                   self.address, self.args[:4]))


class TraceStats:
    def __init__(self):
        self.counts = {}          # kind -> count (unknown kinds counted by id)
        self.unknown_kinds = 0    # records with kind id outside the contract
        self.gap_events = 0       # positions where seq jumped forward
        self.gap_records = 0      # total records missing across all jumps
        self.truncated_tail = 0   # leftover bytes of a torn final record
        self.threads = {}         # thread_id -> record count

    def __str__(self):
        parts = ["%s=%d" % (KIND_NAMES.get(k, "kind_%d" % k), v)
                 for k, v in sorted(self.counts.items())]
        return ("records: %s | gaps: %d jumps/%d records | truncated tail: %dB"
                % (", ".join(parts) or "none", self.gap_events,
                   self.gap_records, self.truncated_tail))


class Trace:
    """Parsed .pbtr file: meta dict + record list + stats."""

    def __init__(self, meta, records, stats):
        self.meta = meta
        self.records = records
        self.stats = stats

    def by_thread(self, thread_id):
        return [r for r in self.records if r.thread_id == thread_id]

    def thread_ids(self):
        return sorted(self.stats.threads)

    def dominant_thread(self):
        """Thread with the most exec/exec_bytes records (the usual window)."""
        best, best_n = None, -1
        per = {}
        for r in self.records:
            if r.kind in (KIND_EXEC, KIND_EXEC_BYTES):
                per[r.thread_id] = per.get(r.thread_id, 0) + 1
        for tid, n in per.items():
            if n > best_n:
                best, best_n = tid, n
        return best


def iter_records(path, skip_unknown=True):
    """Stream records from a .pbtr file (memory-friendly alternative to load()).
    Yields Record objects only; use read_header() or load() for the meta."""
    with open(path, "rb") as fh:
        read_header(fh)
        carry = b""
        while True:
            chunk = fh.read(1 << 20)
            if not chunk and not carry:
                return
            buf = carry + chunk
            off = 0
            while off + RECORD_LEN <= len(buf):
                values = RECORD.unpack_from(buf, off)
                off += RECORD_LEN
                rec = Record(values[0], values[1], values[2], values[3], values[4:])
                if skip_unknown and rec.kind not in KIND_NAMES:
                    continue
                yield rec
            carry = buf[off:]      # truncated tail record: dropped on next EOF
            if not chunk:
                return


def read_header(fh):
    raw = fh.read(HEADER_LEN)
    if len(raw) < HEADER_LEN:
        raise ValueError("file too short for PBTR header")
    magic, version, meta_len, _reserved = HEADER.unpack(raw)
    if magic != MAGIC:
        raise ValueError("bad magic %r (not a .pbtr file)" % magic)
    if version != VERSION:
        raise ValueError("unsupported PBTR version %d" % version)
    meta_raw = fh.read(meta_len)
    if len(meta_raw) < meta_len:
        raise ValueError("truncated meta blob")
    try:
        return json.loads(meta_raw.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        return {"_raw_meta": meta_raw.hex()}


def load(path, thread_id=None, skip_unknown=True):
    """Parse a whole .pbtr file. Returns Trace(meta, records, stats).
    Unknown kinds are skipped (counted in stats); a truncated tail record is
    tolerated (byte count in stats.truncated_tail)."""
    stats = TraceStats()
    records = []
    with open(path, "rb") as fh:
        meta = read_header(fh)
        prev_seq = None
        while True:
            chunk = fh.read(RECORD_LEN)
            if not chunk:
                break
            if len(chunk) < RECORD_LEN:
                stats.truncated_tail = len(chunk)
                break
            values = RECORD.unpack(chunk)
            rec = Record(values[0], values[1], values[2], values[3], values[4:])
            known = rec.kind in KIND_NAMES
            stats.counts[rec.kind] = stats.counts.get(rec.kind, 0) + 1
            stats.threads[rec.thread_id] = stats.threads.get(rec.thread_id, 0) + 1
            if prev_seq is not None and rec.sequence > prev_seq + 1:
                stats.gap_events += 1
                stats.gap_records += rec.sequence - prev_seq - 1
            prev_seq = rec.sequence
            if not known:
                stats.unknown_kinds += 1
                if skip_unknown:
                    continue
            if thread_id is None or rec.thread_id == thread_id:
                records.append(rec)
    return Trace(meta, records, stats)


class TraceWriter:
    """Writes contract-conformant .pbtr files (used by the recorder and tests)."""

    def __init__(self, path, meta):
        self.fh = open(path, "wb")
        blob = json.dumps(meta, separators=(",", ":")).encode("utf-8")
        self.fh.write(HEADER.pack(MAGIC, VERSION, len(blob), 0))
        self.fh.write(blob)
        self.count = 0

    def emit(self, sequence, kind, thread_id, address, *args):
        if len(args) > 8:
            raise ValueError("at most 8 args")
        padded = list(args) + [0] * (8 - len(args))
        self.fh.write(RECORD.pack(sequence, kind, thread_id, address, *padded))
        self.count += 1

    def emit_record(self, record):
        self.emit(record.sequence, record.kind, record.thread_id,
                  record.address, *record.args)

    def close(self):
        self.fh.close()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False


def write_trace(path, meta, records):
    """One-shot writer: records is an iterable of Record (or tuples)."""
    with TraceWriter(path, meta) as writer:
        for rec in records:
            if isinstance(rec, Record):
                writer.emit_record(rec)
            else:
                writer.emit(*rec)
    return path


def main(argv):
    if len(argv) != 2:
        print("usage: python pbtrace.py <trace.pbtr>   # print meta + stats")
        return 2
    trace = load(argv[1])
    print("meta:", json.dumps(trace.meta, indent=2, sort_keys=True))
    print("threads:", {tid: n for tid, n in sorted(trace.stats.threads.items())})
    print(trace.stats)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
