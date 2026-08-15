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
    kind 11 marker:      address=0, arg0=tag, arg1=value;
                         scope-add tag 3 uses arg1=lo, arg2=hi
    kind 12 repeat:      previous payload repeated arg0 additional times;
                         sequence is the final sequence in the run and arg1
                         stores the original kind
    kind 13 reg_snapshot: address=instruction IP, arg0=Pin register id,
                          arg1/arg2=value low/high, arg3=width, arg7=frame id
    kind 5  syscall:      address=RIP, arg0=number, arg1=phase (0=entry,1=exit)
    kind 6  context_change: address=RIP, arg0=reason, arg1=info, arg2=context IP

Readers MUST skip unknown kinds and tolerate a truncated tail record.
Multi-thread: records interleave; split by thread_id for single-thread windows.

Pure stdlib (struct + json).
"""

from __future__ import annotations

import json
import argparse
import struct
import sys
from dataclasses import dataclass, field

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
MARKER_SCOPE_ADD = 3
KIND_REPEAT = 12
KIND_REG_SNAPSHOT = 13

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
    KIND_REPEAT: "repeat",
    KIND_REG_SNAPSHOT: "reg_snapshot",
}
NAME_KINDS = {name: kind for kind, name in KIND_NAMES.items()}

REG_NAMES = {
    7: "rbx", 8: "rdx", 9: "rcx", 10: "rax", 11: "r8", 12: "r9",
    13: "r10", 14: "r11", 15: "r12", 16: "r13", 17: "r14", 18: "r15",
    5: "rbp", 6: "rsp", 3: "rdi", 4: "rsi", 25: "rflags", 26: "rip",
}
# Public x86 wire ids are stable even though Pin's native IA-32 REG enum uses
# a different layout; the C bridge translates these ids before calling Pin.
REG_NAMES.update({
    56: "eax", 53: "ebx", 55: "ecx", 54: "edx", 47: "esi", 45: "edi",
    49: "ebp", 51: "esp", 58: "eip", 57: "eflags",
})
REG_NAMES.update({91 + index: "xmm%d" % index for index in range(32)})
REG_NAMES.update({123 + index: "ymm%d" % index for index in range(32)})
REG_NAMES.update({155 + index: "zmm%d" % index for index in range(32)})


def _assembly_text(code, address, arch="x64"):
    """Decode bytes when Capstone is installed; keep the reader stdlib-only."""
    if not code:
        return ""
    if code == b"\x0f\x05":
        return "syscall"
    try:
        import capstone
        mode = capstone.CS_MODE_32 if arch == "x86" else capstone.CS_MODE_64
        decoder = capstone.Cs(capstone.CS_ARCH_X86, mode)
        instruction = next(decoder.disasm(code, address), None)
        return instruction.mnemonic + (" " + instruction.op_str
                                       if instruction and instruction.op_str else "") \
            if instruction else ""
    except ImportError:
        return ""

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
    def arg7(self): return self.args[7]

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

    def as_dict(self):
        """Typed, JSON-safe projection for scripts, frontends, MCP and AI."""
        out = {
            "seq": self.sequence,
            "kind": self.kind_name,
            "tid": self.thread_id,
            "ip": "0x%x" % self.address,
        }
        if self.kind == KIND_EXEC:
            out["size"] = self.arg0
        elif self.kind == KIND_EXEC_BYTES:
            out["size"] = self.arg0
            out["bytes"] = self.exec_bytes().hex()
        elif self.kind in (KIND_MEMORY, KIND_MEM_VALUE):
            out.update({
                "memory": "0x%x" % self.arg0,
                "size": self.arg1,
                "access": {ACCESS_READ: "read", ACCESS_WRITE: "write",
                           ACCESS_READ2: "read2"}.get(self.arg2,
                                                       "unknown_%d" % self.arg2),
            })
            if self.kind == KIND_MEM_VALUE:
                out["value"] = "0x%x" % self.arg3
        elif self.kind == KIND_BRANCH:
            out["target"] = "0x%x" % self.arg0
            out["taken"] = bool(self.arg1)
        elif self.kind == KIND_SYSCALL:
            out["number"] = self.arg0
            out["phase"] = "exit" if self.arg1 else "entry"
            if self.arg1:
                out["return"] = "0x%x" % self.arg3
                out["errno"] = "0x%x" % self.arg4
            else:
                out["args"] = ["0x%x" % value for value in self.args[2:8]]
        elif self.kind == KIND_CONTEXT_CHANGE:
            out["reason"] = self.arg0
            out["info"] = self.arg1
            out["context_ip"] = "0x%x" % self.arg2
        elif self.kind == KIND_MARKER:
            out["tag"] = self.arg0
            out["value"] = self.arg1
        elif self.kind == KIND_REG_SNAPSHOT:
            out["frame"] = self.arg7
            if self.arg0 == 0:
                out["type"] = "register_header"
                out["mask_lo"] = "0x%016x" % self.arg1
                out["mask_hi"] = "0x%016x" % self.arg2
                out["mode"] = "baseline" if self.arg3 == 1 else "delta"
                return out
            out["reg_id"] = self.arg0
            out["reg"] = REG_NAMES.get(self.arg0, "pin_reg_%d" % self.arg0)
            out["width"] = self.arg3
            out["part"] = self.args[4]
            if self.arg3 >= 16:
                out["value"] = "0x%016x%016x" % (self.arg2, self.arg1)
            else:
                out["value"] = "0x%016x" % self.arg1
        elif self.kind == KIND_REPEAT:
            out["additional_repeats"] = self.arg0
            out["original_kind"] = KIND_NAMES.get(self.arg1,
                                                    "unknown_%d" % self.arg1)
        else:
            out["args"] = ["0x%x" % value for value in self.args]
        return out


class TraceStats:
    def __init__(self):
        self.counts = {}          # kind -> count (unknown kinds counted by id)
        self.physical_counts = {} # kind -> count on disk
        self.physical_records = 0 # 88-byte records on disk
        self.logical_records = 0  # records after repeat expansion
        self.repeat_markers = 0
        self.repeated_records = 0
        self.invalid_repeats = 0
        self.unknown_kinds = 0    # records with kind id outside the contract
        self.gap_events = 0       # positions where seq jumped forward
        self.gap_records = 0      # total records missing across all jumps
        self.truncated_tail = 0   # leftover bytes of a torn final record
        self.threads = {}         # thread_id -> record count

    @property
    def compression_ratio(self):
        """Logical records represented per physical record (1.0 = no gain)."""
        if not self.physical_records:
            return 1.0
        return float(self.logical_records) / self.physical_records

    def __str__(self):
        parts = ["%s=%d" % (KIND_NAMES.get(k, "kind_%d" % k), v)
                 for k, v in sorted(self.counts.items())]
        return ("logical=%d physical=%d repeat_markers=%d repeated=%d "
                "ratio=%.2fx | %s | gaps: %d jumps/%d records | truncated tail: %dB"
                % (self.logical_records, self.physical_records,
                   self.repeat_markers, self.repeated_records,
                   self.compression_ratio, ", ".join(parts) or "none",
                   self.gap_events, self.gap_records, self.truncated_tail))


@dataclass
class TraceFrame:
    """Logical instruction window assembled from physical PBTR records."""
    sequence: int
    thread_id: int
    address: int
    size: int = 0
    machine_code: bytes = b""
    assembly: str = ""
    registers: dict = field(default_factory=dict)
    register_changes: dict = field(default_factory=dict)
    memory: list = field(default_factory=list)
    branches: list = field(default_factory=list)
    syscalls: list = field(default_factory=list)
    exceptions: list = field(default_factory=list)
    records: list = field(default_factory=list)
    frame_id: int = 0
    _context_mode: int = field(default=0, repr=False)
    _context_parts: dict = field(default_factory=dict, repr=False)
    _context_widths: dict = field(default_factory=dict, repr=False)
    _context_masks: tuple = field(default=(0, 0), repr=False)
    context_complete: bool = True

    def as_dict(self):
        return {
            "seq": self.sequence,
            "tid": self.thread_id,
            "ip": "0x%x" % self.address,
            "size": self.size,
            "bytes": self.machine_code.hex(),
            "asm": self.assembly or None,
            "registers": self.registers,
            "register_changes": self.register_changes,
            "memory": self.memory,
            "branches": self.branches,
            "syscalls": self.syscalls,
            "exceptions": self.exceptions,
            "frame": self.frame_id,
        }


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

    def frames(self, thread_id=None):
        """Assemble instruction-oriented TraceFrame objects.

        Kind-13 register headers/components are joined by frame id. Other
        event kinds are attached to the most recent instruction for the same
        thread and IP, which matches the recorder insertion order.
        """
        output = []
        active = {}
        context_frames = {}
        register_state = {}
        pending_syscall = {}

        def apply_context(frame):
            expected_registers = (bin(frame._context_masks[0]).count("1") +
                                  bin(frame._context_masks[1]).count("1"))
            if len(frame._context_parts) != expected_registers:
                frame.context_complete = False
                return
            for reg_id, parts in frame._context_parts.items():
                width = frame._context_widths.get(reg_id, 0)
                expected_parts = 1 if 0 < width <= 8 else width // 16
                if (expected_parts == 0 or
                        set(parts) != set(range(expected_parts))):
                    frame.context_complete = False
                    return
            state = register_state.setdefault(frame.thread_id, {})
            if frame._context_mode == 1:
                state.clear()
            for reg_id, parts in frame._context_parts.items():
                data = b"".join(parts[index] for index in sorted(parts))
                name = REG_NAMES.get(reg_id, "pin_reg_%d" % reg_id)
                state[reg_id] = data
                value = "0x%x" % int.from_bytes(data, "little")
                frame.register_changes[name] = value
            frame.registers = {
                REG_NAMES.get(reg_id, "pin_reg_%d" % reg_id):
                "0x%x" % int.from_bytes(data, "little")
                for reg_id, data in state.items()
            }

        def new_frame(rec):
            frame = TraceFrame(rec.sequence, rec.thread_id, rec.address)
            output.append(frame)
            active[rec.thread_id] = frame
            return frame

        for rec in self.records:
            if thread_id is not None and rec.thread_id != thread_id:
                continue
            if rec.kind == KIND_MARKER:
                continue
            if rec.kind == KIND_REG_SNAPSHOT:
                key = (rec.thread_id, rec.arg7)
                if rec.arg0 == 0:
                    frame = TraceFrame(rec.sequence, rec.thread_id, rec.address,
                                       frame_id=rec.arg7)
                    frame._context_mode = rec.arg3
                    frame._context_masks = (rec.arg1, rec.arg2)
                    context_frames[key] = frame
                    output.append(frame)
                    active[rec.thread_id] = frame
                else:
                    frame = context_frames.get(key)
                    if frame is not None:
                        width = rec.arg3
                        if width <= 8:
                            payload = struct.pack("<Q", rec.arg1)[:width]
                        else:
                            payload = struct.pack("<QQ", rec.arg1, rec.arg2)
                        frame._context_parts.setdefault(rec.arg0, {})[
                            rec.args[4]] = payload
                        frame._context_widths[rec.arg0] = width
                        frame.records.append(rec)
                continue

            if (rec.kind == KIND_SYSCALL and rec.arg1 == 1 and
                    rec.thread_id in pending_syscall):
                frame = pending_syscall.pop(rec.thread_id)
            else:
                frame = active.get(rec.thread_id)
            starts_instruction = rec.kind in (KIND_EXEC, KIND_EXEC_BYTES)
            syscall_exit = rec.kind == KIND_SYSCALL and rec.arg1 == 1
            repeats_start_kind = (starts_instruction and frame is not None and
                                  any(item.kind == rec.kind
                                      for item in frame.records))
            if (frame is None or (frame.address != rec.address and not syscall_exit) or
                    repeats_start_kind):
                frame = new_frame(rec)
            if frame.frame_id:
                apply_context(frame)
            frame.records.append(rec)
            if rec.kind == KIND_EXEC:
                frame.sequence = rec.sequence
                frame.size = rec.arg0
            elif rec.kind == KIND_EXEC_BYTES:
                frame.sequence = rec.sequence
                frame.size = rec.arg0
                frame.machine_code = rec.exec_bytes()
                frame.assembly = _assembly_text(
                    frame.machine_code, frame.address,
                    self.meta.get("arch", "x64"))
            elif rec.kind in (KIND_MEMORY, KIND_MEM_VALUE):
                frame.memory.append(rec.as_dict())
            elif rec.kind == KIND_BRANCH:
                frame.branches.append(rec.as_dict())
            elif rec.kind == KIND_SYSCALL:
                frame.syscalls.append(rec.as_dict())
                if rec.arg1 == 0:
                    pending_syscall[rec.thread_id] = frame
            elif rec.kind == KIND_CONTEXT_CHANGE:
                frame.exceptions.append(rec.as_dict())

        for frame in output:
            if frame.frame_id:
                apply_context(frame)
        return output


def _repeat_count(previous, repeat):
    """Return the expansion count for a valid kind-12 marker, else None."""
    count = repeat.arg0
    if (previous is None or count == 0 or repeat.arg1 != previous.kind or
            repeat.thread_id != previous.thread_id or
            repeat.address != previous.address or
            repeat.sequence != previous.sequence + count):
        return None
    return count


def iter_records(path, skip_unknown=True, expand_repeats=True):
    """Stream records from a .pbtr file (memory-friendly alternative to load()).
    Yields Record objects only; use read_header() or load() for the meta."""
    previous = None
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
                if expand_repeats and rec.kind == KIND_REPEAT:
                    count = _repeat_count(previous, rec)
                    if count is not None:
                        for offset in range(1, count + 1):
                            expanded = Record(previous.sequence + offset,
                                              previous.kind, previous.thread_id,
                                              previous.address, previous.args)
                            yield expanded
                        continue
                yield rec
                if rec.kind != KIND_REPEAT:
                    previous = rec
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


def load(path, thread_id=None, skip_unknown=True, expand_repeats=True,
         collect_records=True):
    """Parse a whole .pbtr file. Returns Trace(meta, records, stats).
    Unknown kinds are skipped (counted in stats); a truncated tail record is
    tolerated (byte count in stats.truncated_tail)."""
    stats = TraceStats()
    records = []
    scope_additions = []
    with open(path, "rb") as fh:
        meta = read_header(fh)
        prev_seq = None
        previous = None
        while True:
            chunk = fh.read(RECORD_LEN)
            if not chunk:
                break
            if len(chunk) < RECORD_LEN:
                stats.truncated_tail = len(chunk)
                break
            values = RECORD.unpack(chunk)
            rec = Record(values[0], values[1], values[2], values[3], values[4:])
            if (rec.kind == KIND_MARKER and rec.arg0 == MARKER_SCOPE_ADD):
                scope_additions.append([rec.arg1, rec.arg2])
            known = rec.kind in KIND_NAMES
            stats.physical_records += 1
            stats.physical_counts[rec.kind] = stats.physical_counts.get(rec.kind, 0) + 1
            if rec.kind == KIND_REPEAT:
                stats.repeat_markers += 1
                count = _repeat_count(previous, rec)
                if count is None:
                    stats.invalid_repeats += 1
                    if prev_seq is not None and rec.sequence > prev_seq + 1:
                        stats.gap_events += 1
                        stats.gap_records += rec.sequence - prev_seq - 1
                    prev_seq = rec.sequence
                    if collect_records and not (skip_unknown and not known):
                        if thread_id is None or rec.thread_id == thread_id:
                            records.append(rec)
                    stats.counts[rec.kind] = stats.counts.get(rec.kind, 0) + 1
                    stats.logical_records += 1
                    stats.threads[rec.thread_id] = stats.threads.get(rec.thread_id, 0) + 1
                    continue
                stats.repeated_records += count
                stats.logical_records += count
                stats.counts[previous.kind] = stats.counts.get(previous.kind, 0) + count
                stats.threads[previous.thread_id] = stats.threads.get(previous.thread_id, 0) + count
                if (collect_records and expand_repeats and
                        not (skip_unknown and previous.kind not in KIND_NAMES)):
                    if thread_id is None or previous.thread_id == thread_id:
                        for offset in range(1, count + 1):
                            records.append(Record(previous.sequence + offset,
                                                  previous.kind,
                                                  previous.thread_id,
                                                  previous.address,
                                                  previous.args))
                elif (collect_records and not expand_repeats and
                      not (skip_unknown and not known)):
                    if thread_id is None or rec.thread_id == thread_id:
                        records.append(rec)
                prev_seq = rec.sequence
                continue
            if prev_seq is not None and rec.sequence > prev_seq + 1:
                stats.gap_events += 1
                stats.gap_records += rec.sequence - prev_seq - 1
            prev_seq = rec.sequence
            stats.counts[rec.kind] = stats.counts.get(rec.kind, 0) + 1
            stats.logical_records += 1
            stats.threads[rec.thread_id] = stats.threads.get(rec.thread_id, 0) + 1
            if not known:
                stats.unknown_kinds += 1
                if skip_unknown:
                    continue
            if collect_records and (thread_id is None or rec.thread_id == thread_id):
                records.append(rec)
            previous = rec
    if scope_additions:
        meta = dict(meta)
        meta["scope_additions"] = scope_additions
    return Trace(meta, records, stats)


class TraceWriter:
    """Writes contract-conformant .pbtr files.

    ``compress_repeats=True`` buffers one logical run and emits the same
    lossless kind-12 encoding as the native recorder. It is opt-in so callers
    that inspect physical records retain the historical behavior.
    """

    def __init__(self, path, meta, compress_repeats=False):
        self.fh = open(path, "wb")
        blob = json.dumps(meta, separators=(",", ":")).encode("utf-8")
        self.fh.write(HEADER.pack(MAGIC, VERSION, len(blob), 0))
        self.fh.write(blob)
        self.count = 0
        self.physical_count = 0
        self.compress_repeats = compress_repeats
        self._pending = None
        self._pending_repeats = 0

    @staticmethod
    def _same_payload(left, right):
        return (left.kind == right.kind and left.thread_id == right.thread_id and
                left.address == right.address and left.args == right.args)

    def _write_record(self, record):
        self.fh.write(RECORD.pack(record.sequence, record.kind,
                                  record.thread_id, record.address,
                                  *record.args))
        self.physical_count += 1

    def _flush_pending(self):
        if self._pending is None:
            return
        base = self._pending
        self._write_record(base)
        if self._pending_repeats:
            repeat = Record(base.sequence + self._pending_repeats,
                            KIND_REPEAT, base.thread_id, base.address,
                            (self._pending_repeats, base.kind, 0, 0, 0, 0, 0, 0))
            self._write_record(repeat)
        self._pending = None
        self._pending_repeats = 0

    def emit(self, sequence, kind, thread_id, address, *args):
        if len(args) > 8:
            raise ValueError("at most 8 args")
        padded = list(args) + [0] * (8 - len(args))
        record = Record(sequence, kind, thread_id, address, padded)
        if self.compress_repeats:
            if (self._pending is not None and
                    sequence == self._pending.sequence + self._pending_repeats + 1 and
                    self._same_payload(self._pending, record)):
                self._pending_repeats += 1
            else:
                self._flush_pending()
                self._pending = record
        else:
            self._write_record(record)
        self.count += 1

    def emit_record(self, record):
        self.emit(record.sequence, record.kind, record.thread_id,
                  record.address, *record.args)

    def close(self):
        if not self.fh.closed:
            self._flush_pending()
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
    parser = argparse.ArgumentParser(
        description="inspect PBTR metadata/stats and emit an AI-readable JSONL preview")
    parser.add_argument("trace")
    parser.add_argument("--records", type=int, default=0,
                        help="emit the first N records as typed JSONL")
    parser.add_argument("--expanded", action="store_true",
                        help="expand kind-12 runs in the JSONL preview")
    parser.add_argument("--frames", type=int, default=0,
                        help="emit the first N assembled TraceFrame JSON objects")
    args = parser.parse_args(argv[1:])
    # Stats scan stays O(1) memory even for multi-gigabyte captures.
    trace = load(args.trace, expand_repeats=args.frames > 0,
                 collect_records=args.frames > 0)
    print("meta:", json.dumps(trace.meta, indent=2, sort_keys=True))
    print("threads:", {tid: n for tid, n in sorted(trace.stats.threads.items())})
    print(trace.stats)
    if args.records > 0:
        print("records_jsonl:")
        for index, rec in enumerate(iter_records(
                args.trace, expand_repeats=args.expanded)):
            if index >= args.records:
                break
            print(json.dumps(rec.as_dict(), separators=(",", ":"),
                             sort_keys=True))
    if args.frames > 0:
        print("frames_jsonl:")
        for frame in trace.frames()[:args.frames]:
            print(json.dumps(frame.as_dict(), separators=(",", ":"),
                             sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
