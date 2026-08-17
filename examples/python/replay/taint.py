#!/usr/bin/env python3
"""taint.py — offline taint replay over .pbtr recordings (pure Python + capstone).

Architecture (docs/taint-roadmap.md layer 3): native engines RECORD a window at
full speed; taint forward propagation and backward slicing run OFFLINE here on
the recording. Concrete EAs from the memory events kill the aliasing problem:
no pointer analysis, no guessing.

Instruction bytes:
  * kind-9 exec_bytes records carry the bytes inline (preferred, SMC-safe);
  * otherwise fall back to kind-3 exec records + `--pe module.exe [--base 0x..]`
    to map instruction bytes from the on-disk PE. CAVEAT: this breaks on SMC /
    self-decrypting code — the on-disk bytes are not the executed bytes. Use a
    档2 (kind-9/10) recording for packed targets.

Taint model (v1):
  * register taint at BYTE granularity (8-byte x64 or 4-byte x86 GP banks);
    in x64, a 32-bit subregister write zero-extends and therefore KILLS the
    upper 4 bytes' taint;
  * shadow memory at byte granularity keyed by concrete EA;
  * labels = source ids (s0, s1, ...); each tainted byte also tracks a
    "chain depth" = longest propagation path from a source (reported as the
    provenance chain length at sinks);
  * value-blind: 档1 recordings carry no data values, so taint is computed from
    instruction semantics only (e.g. `and eax, 0` still propagates taint).
  * single-thread windows (records are split by thread_id).

Usage:
  python taint.py trace.pbtr forward --source mem:0x..:0x40 [--source reg:RAX/EAX]
                                     [--sink reg:RIP/EIP] [--sink mem:0xLO-0xHI]
                                     [--thread N] [--max-events N] [--pe f --base B]
  python taint.py trace.pbtr slice --at 12345 --operand reg:rdx [--thread N]
"""

from __future__ import annotations

import argparse
import struct
import sys

import pbtrace

try:
    import capstone
    from capstone.x86 import X86_OP_REG, X86_OP_MEM, X86_OP_IMM
except ImportError:
    capstone = None
    X86_OP_REG, X86_OP_MEM, X86_OP_IMM = 1, 2, 3


# ---------------------------------------------------------------------------
# architecture/register model (byte-granular GP file + instruction pointer)
# ---------------------------------------------------------------------------

# classic naming order: rax rcx rdx rbx rsp rbp rsi rdi
_N32 = ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"]
_N16 = ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"]
_N8L = ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"]
_GP64 = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"] + \
        ["r%d" % i for i in range(8, 16)]


class Architecture:
    """Instruction/register semantics selected from PBTR metadata."""

    def __init__(self, name="x64"):
        normalized = str(name or "x64").strip().lower()
        if normalized in ("x86", "ia32", "i386", "i686", "32"):
            self.name = "x86"
            self.pointer_size = 4
        elif normalized in ("x64", "amd64", "x86_64", "ia32e", "64"):
            self.name = "x64"
            self.pointer_size = 8
        else:
            raise SystemExit("unsupported trace architecture: %s" % name)
        self.zero_extends_32 = self.name == "x64"
        self.reg_info = self._build_registers()
        self.ip = "rip" if self.name == "x64" else "eip"
        self.sp = "rsp" if self.name == "x64" else "esp"
        self.bp = "rbp" if self.name == "x64" else "ebp"

    def _build_registers(self):
        info = {}
        if self.name == "x64":
            for base in _GP64:
                info[base] = (base, 0, 8)
            for index in range(8):
                info[_N32[index]] = (_GP64[index], 0, 4)
                info[_N16[index]] = (_GP64[index], 0, 2)
                info[_N8L[index]] = (_GP64[index], 0, 1)
            for index in range(8, 16):
                info["r%dd" % index] = ("r%d" % index, 0, 4)
                info["r%dw" % index] = ("r%d" % index, 0, 2)
                info["r%db" % index] = ("r%d" % index, 0, 1)
            high_bases = ("rax", "rcx", "rdx", "rbx")
            info["rip"] = ("rip", 0, 8)
            info["eip"] = ("rip", 0, 4)
        else:
            for index, base in enumerate(_N32):
                info[base] = (base, 0, 4)
                info[_N16[index]] = (base, 0, 2)
                if index < 4:
                    info[_N8L[index]] = (base, 0, 1)
            high_bases = ("eax", "ecx", "edx", "ebx")
            info["eip"] = ("eip", 0, 4)
        for name, base in zip(("ah", "ch", "dh", "bh"), high_bases):
            info[name] = (base, 1, 1)
        return info

    def bank_size(self, base):
        return max(size + offset for item_base, offset, size in self.reg_info.values()
                   if item_base == base)


DEFAULT_ARCH = Architecture("x64")
# Compatibility alias for callers that inspect the original x64 table.
REG_INFO = DEFAULT_ARCH.reg_info


def architecture_from_meta(meta):
    name = meta.get("arch") if meta else None
    if not name and meta and meta.get("pointer_width") == 4:
        name = "x86"
    return Architecture(name or "x64")

# taint entry: (frozenset_of_labels, chain_depth); clean = (frozenset(), 0)
CLEAN = (frozenset(), 0)


def taint_union(entries):
    labels = set()
    depth = 0
    for labs, dep in entries:
        labels |= labs
        if dep > depth:
            depth = dep
    if not labels:
        return CLEAN
    return (frozenset(labels), depth)


class RegState:
    """Per-thread register taint: canonical base -> byte entries."""

    def __init__(self, arch=None):
        self.arch = arch or DEFAULT_ARCH
        self.regs = {}

    def _bank(self, base):
        bank = self.regs.get(base)
        if bank is None:
            bank = [CLEAN] * self.arch.bank_size(base)
            self.regs[base] = bank
        return bank

    def read(self, name):
        base, off, size = self.arch.reg_info[name]
        bank = self.regs.get(base)
        if bank is None:
            return CLEAN
        return taint_union(bank[off:off + size])

    def write(self, name, entry):
        base, off, size = self.arch.reg_info[name]
        bank = self._bank(base)
        if self.arch.zero_extends_32 and size == 4 and off == 0:
            # 32-bit writes zero-extend: upper 4 bytes become a clean constant
            for i in range(4):
                bank[i] = entry
            for i in range(4, 8):
                bank[i] = CLEAN
        else:
            for i in range(off, off + size):
                bank[i] = entry

    def reg_bytes(self, name):
        """byte keys covered by a read of this register name"""
        base, off, size = self.arch.reg_info[name]
        return {("r", base, off + i) for i in range(size)}

    def reg_def_bytes(self, name):
        """(full, val) byte keys for a write: full includes the zero-extended
        upper half of 32-bit writes; val is where source taint lands."""
        base, off, size = self.arch.reg_info[name]
        val = {("r", base, off + i) for i in range(size)}
        if self.arch.zero_extends_32 and size == 4 and off == 0:
            full = val | {("r", base, i) for i in range(4, 8)}
        else:
            full = val
        return full, val


# ---------------------------------------------------------------------------
# instruction decode (capstone; kind-9 bytes preferred, --pe fallback)
# ---------------------------------------------------------------------------

class PEImage:
    """Minimal PE32/PE32+ mapper for the kind-3 fallback path (no SMC!)."""

    def __init__(self, path, base=None, arch=None):
        with open(path, "rb") as fh:
            self.data = fh.read()
        if self.data[:2] != b"MZ":
            raise SystemExit("--pe: not a PE file: %s" % path)
        e_lfanew = struct.unpack_from("<I", self.data, 0x3C)[0]
        if self.data[e_lfanew:e_lfanew + 4] != b"PE\0\0":
            raise SystemExit("--pe: bad PE signature")
        opt = e_lfanew + 24
        magic = struct.unpack_from("<H", self.data, opt)[0]
        if magic == 0x20B:
            self.arch = Architecture("x64")
            image_base = struct.unpack_from("<Q", self.data, opt + 24)[0]
        elif magic == 0x10B:
            self.arch = Architecture("x86")
            image_base = struct.unpack_from("<I", self.data, opt + 28)[0]
        else:
            raise SystemExit("--pe: unsupported optional-header magic 0x%x" % magic)
        if arch is not None and self.arch.name != arch.name:
            raise SystemExit("--pe architecture %s does not match trace %s" %
                             (self.arch.name, arch.name))
        self.base = base if base is not None else image_base
        size_opt = struct.unpack_from("<H", self.data, e_lfanew + 20)[0]
        nsec = struct.unpack_from("<H", self.data, e_lfanew + 6)[0]
        self.sections = []
        sec = opt + size_opt
        for _ in range(nsec):
            name = self.data[sec:sec + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, vaddr, rawsize, rawptr = struct.unpack_from("<IIII", self.data, sec + 8)
            self.sections.append((name, vaddr, vsize, rawptr, rawsize))
            sec += 40

    def read(self, va, count):
        rva = va - self.base
        if rva < 0:
            return None
        for _name, vaddr, vsize, rawptr, rawsize in self.sections:
            if vaddr <= rva < vaddr + max(vsize, rawsize):
                off = rawptr + (rva - vaddr)
                return self.data[off:off + count]
        return None


class DecodedInsn:
    __slots__ = ("ip", "size", "mnemonic", "op_str", "operands", "ok")

    def __init__(self, ip, size, mnemonic, op_str, operands, ok=True):
        self.ip = ip
        self.size = size
        self.mnemonic = mnemonic
        self.op_str = op_str
        self.operands = operands      # list of Op
        self.ok = ok

    @property
    def text(self):
        return (self.mnemonic + (" " + self.op_str if self.op_str else "")).strip()


class Op:
    __slots__ = ("kind", "reg", "mem", "imm", "access", "size", "ea")

    def __init__(self, kind, reg=None, mem=None, imm=0, access=0, size=0):
        self.kind = kind              # 'reg' | 'mem' | 'imm'
        self.reg = reg                # canonical name (lowercase)
        self.mem = mem                # (base|None, index|None, scale, disp)
        self.imm = imm
        self.access = access          # CS_AC_READ / CS_AC_WRITE (may be 0)
        self.size = size              # operand size in bytes
        self.ea = None                # concrete EA bound from the mem events


CS_AC_READ = 1
CS_AC_WRITE = 2


class Decoder:
    def __init__(self, pe_path=None, pe_base=None, arch=None):
        if capstone is None:
            raise SystemExit(
                "capstone is required: pip install capstone "
                "(or -i https://mirrors.aliyun.com/pypi/simple/)")
        self.arch = arch if isinstance(arch, Architecture) else Architecture(arch or "x64")
        mode = capstone.CS_MODE_32 if self.arch.name == "x86" else capstone.CS_MODE_64
        self.cs = capstone.Cs(capstone.CS_ARCH_X86, mode)
        self.cs.detail = True
        self.pe = PEImage(pe_path, pe_base, self.arch) if pe_path else None
        self.cache = {}
        self.undecoded = 0

    def decode(self, ip, rec):
        data = None
        if rec is not None and rec.kind == pbtrace.KIND_EXEC_BYTES:
            data = rec.exec_bytes()
        elif self.pe is not None:
            data = self.pe.read(ip, 16)
        # SMC-safe: the same IP may carry different kind-9 bytes at different
        # times. A cache keyed only by IP silently reused stale semantics.
        cache_key = (ip, data)
        hit = self.cache.get(cache_key)
        if hit is not None:
            return hit
        if not data:
            self.undecoded += 1
            insn = DecodedInsn(ip, rec.arg0 if rec else 0, "??", "", [], ok=False)
        else:
            out = None
            for ci in self.cs.disasm(data, ip, count=1):
                ops = []
                for op in ci.operands:
                    if op.type == X86_OP_REG:
                        ops.append(Op("reg", reg=self.cs.reg_name(op.reg),
                                      access=op.access, size=op.size))
                    elif op.type == X86_OP_MEM:
                        m = op.mem
                        base = self.cs.reg_name(m.base) if m.base else None
                        index = self.cs.reg_name(m.index) if m.index else None
                        ops.append(Op("mem", mem=(base, index, m.scale, m.disp),
                                      access=op.access, size=op.size))
                    elif op.type == X86_OP_IMM:
                        ops.append(Op("imm", imm=op.imm, access=op.access,
                                      size=op.size))
                out = DecodedInsn(ip, ci.size, ci.mnemonic, ci.op_str, ops)
                break
            if out is None:
                self.undecoded += 1
                out = DecodedInsn(ip, len(data), "??", data.hex(), [], ok=False)
            insn = out
        self.cache[cache_key] = insn
        return insn


# ---------------------------------------------------------------------------
# window construction: pair exec records with their memory events
# ---------------------------------------------------------------------------

class InsnInstance:
    __slots__ = ("rec", "events", "ip", "seq", "idx", "decoded", "mem_indices",
                 "transfer")

    def __init__(self, rec, idx):
        self.rec = rec
        self.ip = rec.address
        self.seq = rec.sequence
        self.idx = idx
        self.events = []        # memory / mem_value records, in order
        self.mem_indices = []   # window-local memory event index per event
        self.decoded = None


class Window:
    def __init__(self, insns, tid, unpaired, consumed):
        self.insns = insns
        self.tid = tid
        self.unpaired_mem = unpaired
        self.records_consumed = consumed


def build_window(trace, tid, max_events=None):
    if tid is None:
        tid = trace.dominant_thread()
    if tid is None:
        raise SystemExit("no exec records in trace")
    insns = []
    cur = None
    unpaired = 0
    mem_index = 0
    consumed = 0
    for rec in trace.records:
        if max_events is not None and consumed >= max_events:
            break
        consumed += 1
        if rec.thread_id != tid:
            continue
        if rec.kind in (pbtrace.KIND_EXEC, pbtrace.KIND_EXEC_BYTES):
            cur = InsnInstance(rec, len(insns))
            insns.append(cur)
        elif rec.kind in (pbtrace.KIND_MEMORY, pbtrace.KIND_MEM_VALUE):
            # memory records follow their instruction's exec record per thread
            if cur is not None and rec.address == cur.ip:
                cur.events.append(rec)
                cur.mem_indices.append(mem_index)
            else:
                unpaired += 1
            mem_index += 1
    return Window(insns, tid, unpaired, consumed)


# ---------------------------------------------------------------------------
# transfer rules: (def_full, def_val, uses) byte-key steps per instruction
# ---------------------------------------------------------------------------

ALU_BINARY = {"add", "sub", "and", "or", "xor", "adc", "sbb",
              "shl", "shr", "sar", "sal", "rol", "ror"}
MOV_LIKE = {"mov", "movzx", "movsx", "movsxd"}
UNARY = {"neg", "not", "inc", "dec"}
NO_TAINT = {"cmp", "test", "bt", "nop"}


class Step:
    __slots__ = ("def_full", "def_val", "uses")

    def __init__(self, def_full, def_val, uses):
        self.def_full = def_full      # byte keys killed/overwritten
        self.def_val = def_val        # byte keys receiving union(uses)
        self.uses = uses              # byte keys read


class Transfer:
    __slots__ = ("steps", "cf_uses", "known")

    def __init__(self):
        self.steps = []
        self.cf_uses = set()          # byte keys feeding a control-flow target
        self.known = True


def _mem_keys(ea, size):
    return {("m", ea + i) for i in range(size)}


def _infer_access(mn, nops, idx):
    """operand access fallback when capstone leaves it UNKNOWN."""
    if mn in MOV_LIKE or mn.startswith("cmov"):
        return CS_AC_WRITE if idx == 0 else CS_AC_READ
    if mn == "lea":
        return CS_AC_WRITE if idx == 0 else CS_AC_READ
    if mn in ("cmp", "test", "bt"):
        return CS_AC_READ
    if mn in ("push", "call", "jmp"):
        return CS_AC_READ
    if mn == "pop":
        return CS_AC_WRITE
    if mn in ALU_BINARY or mn in ("imul",):
        return (CS_AC_READ | CS_AC_WRITE) if idx == 0 else CS_AC_READ
    if mn in UNARY or mn == "xchg":
        return CS_AC_READ | CS_AC_WRITE
    return (CS_AC_READ | CS_AC_WRITE) if idx == 0 else CS_AC_READ


def compute_transfer(insn, events, arch=None):
    """Build taint transfer steps for one decoded instruction.

    events: resolved memory events already split into reads/writes with
    concrete EAs (see resolve_events). Unknown mnemonics take the
    conservative path: every written operand = union of every read operand.
    """
    arch = arch or DEFAULT_ARCH
    reg_info = arch.reg_info
    mn = insn.mnemonic
    ops = insn.operands
    t = Transfer()

    def op_read_keys(op):
        if op.kind == "reg":
            base, off, size = reg_info.get(op.reg, (None, 0, 0))
            if base is None:
                return set()
            return {("r", base, off + i) for i in range(size)}
        if op.kind == "mem":
            return _mem_keys(op.ea, op.size) if op.ea is not None else set()
        return set()

    def op_def_keys(op):
        if op.kind == "reg":
            base, off, size = reg_info.get(op.reg, (None, 0, 0))
            if base is None:
                return set(), set()
            val = {("r", base, off + i) for i in range(size)}
            if arch.zero_extends_32 and size == 4 and off == 0:
                full = val | {("r", base, i) for i in range(4, 8)}
            else:
                full = val
            return full, val
        if op.kind == "mem":
            keys = _mem_keys(op.ea, op.size) if op.ea is not None else set()
            return keys, keys
        return set(), set()

    def access_of(i):
        acc = ops[i].access
        return acc if acc else _infer_access(mn, len(ops), i)

    def emit(def_op=None, use_ops=(), kill=False, extra_uses=()):
        full, val = op_def_keys(def_op) if def_op is not None else (set(), set())
        uses = set()
        for u in use_ops:
            uses |= op_read_keys(u)
        for u in extra_uses:
            uses |= u
        if kill:
            uses = set()
            val = set()
        t.steps.append(Step(full, val, uses))

    stack_read = events["read_keys"]
    stack_write = events["write_keys"]

    if mn in NO_TAINT or mn == "prefetch":
        return t

    if mn == "??" or not insn.ok:
        t.known = False
        return t

    if mn in MOV_LIKE or mn.startswith("cmov"):
        if len(ops) >= 2:
            uses = [ops[1]] + ([ops[0]] if mn.startswith("cmov") else [])
            emit(ops[0], uses)
        return t

    if mn == "lea":
        if len(ops) >= 2 and ops[1].kind == "mem":
            base, index, _scale, _disp = ops[1].mem
            extra = set()
            for rn in (base, index):
                if rn and rn in reg_info:
                    b, o, s = reg_info[rn]
                    extra |= {("r", b, o + i) for i in range(s)}
            emit(ops[0], (), extra_uses=extra)
        return t

    if mn in ALU_BINARY:
        if len(ops) >= 2:
            same_reg = (ops[0].kind == ops[1].kind == "reg"
                        and ops[0].reg == ops[1].reg)
            kill = mn in ("xor", "sub") and same_reg
            emit(ops[0], [ops[0], ops[1]], kill=kill)
        return t

    if mn == "imul":
        if ops:
            emit(ops[0], list(ops))
        return t

    if mn in UNARY:
        if ops:
            emit(ops[0], [ops[0]])
        return t

    if mn == "xchg":
        if len(ops) >= 2:
            emit(ops[0], [ops[1]])
            emit(ops[1], [ops[0]])
        return t

    if mn == "push" or mn.startswith("push"):
        if ops:
            # def: concrete stack write event; use: the pushed operand
            t.steps.append(Step(set(stack_write), set(stack_write),
                                op_read_keys(ops[0])))
        return t

    if mn == "pop":
        if ops:
            t.steps.append(Step(*op_def_keys(ops[0]), set(stack_read)))
        return t

    if mn == "call":
        # return-address push is a clean constant; target operand is a cf use
        t.steps.append(Step(set(stack_write), set(), set()))
        if ops:
            t.cf_uses = op_read_keys(ops[0])
            if ops[0].kind == "mem" and ops[0].ea is not None:
                t.cf_uses = _mem_keys(ops[0].ea, ops[0].size)
        return t

    if mn == "ret" or mn.startswith("ret"):
        t.cf_uses = set(stack_read)
        return t

    if mn == "jmp":
        if ops:
            t.cf_uses = op_read_keys(ops[0])
        return t

    if mn.startswith("j"):   # jcc: target is an immediate in practice
        if ops and ops[0].kind != "imm":
            t.cf_uses = op_read_keys(ops[0])
        return t

    if mn == "leave":
        # sp <- bp; bp <- [sp], using the trace architecture's pointer width.
        bp_keys = {("r", arch.bp, i) for i in range(arch.pointer_size)}
        sp_keys = {("r", arch.sp, i) for i in range(arch.pointer_size)}
        t.steps.append(Step(set(sp_keys), set(sp_keys), set(bp_keys)))
        t.steps.append(Step(set(bp_keys), set(bp_keys), set(stack_read)))
        return t

    # conservative fallback: every written operand = union of read operands
    t.known = False
    uses = set()
    defs = []
    for i, op in enumerate(ops):
        acc = access_of(i)
        if acc & CS_AC_READ:
            uses |= op_read_keys(op)
        if acc & CS_AC_WRITE and op.kind != "imm":
            defs.append(op)
    for d in defs:
        full, val = op_def_keys(d)
        t.steps.append(Step(full, val, set(uses)))
    return t


# ---------------------------------------------------------------------------
# sources & sinks
# ---------------------------------------------------------------------------

class Source:
    def __init__(self, label, spec, arch=None):
        self.arch = arch or DEFAULT_ARCH
        self.label = label
        self.kind = None
        self.reg = None
        self.lo = self.hi = None
        self.when = "first-touch"
        self.event_index = None
        self._parse(spec)

    def _parse(self, spec):
        when = None
        if "@" in spec:
            spec, when = spec.split("@", 1)
        if spec.startswith("reg:"):
            self.kind = "reg"
            self.reg = spec[4:].lower()
            if self.reg not in self.arch.reg_info:
                raise SystemExit("unknown %s source register: %s" %
                                 (self.arch.name, self.reg))
        elif spec.startswith("mem:"):
            self.kind = "mem"
            parts = spec[4:].split(":")
            if len(parts) != 2:
                raise SystemExit("mem source syntax: mem:0xADDR:0xSIZE[@when]")
            self.lo = int(parts[0], 0)
            self.hi = self.lo + int(parts[1], 0)
            self.when = when or "first-touch"
            if self.when not in ("first-touch", "start"):
                raise SystemExit("mem source when := first-touch|start")
        elif spec.startswith("event:#"):
            self.kind = "event"
            self.event_index = int(spec[7:], 0)
        else:
            raise SystemExit("unknown source spec: %s" % spec)

    def describe(self):
        if self.kind == "reg":
            return "%s: reg:%s@entry" % (self.label, self.reg)
        if self.kind == "mem":
            return "%s: mem:0x%x-0x%x@%s" % (self.label, self.lo, self.hi, self.when)
        return "%s: event:#%d" % (self.label, self.event_index)


def parse_sink(spec, arch=None):
    arch = arch or DEFAULT_ARCH
    if spec.startswith("reg:"):
        name = spec[4:].lower()
        if name not in arch.reg_info:
            raise SystemExit("unknown %s sink register: %s" % (arch.name, name))
        return ("reg", name)
    if spec.startswith("mem:"):
        rng = spec[4:]
        sep = "-" if "-" in rng else ":"
        lo_s, hi_s = rng.split(sep, 1)
        return ("mem", int(lo_s, 0), int(hi_s, 0))
    raise SystemExit("unknown sink spec: %s (want reg:NAME or mem:LO-HI)" % spec)


# ---------------------------------------------------------------------------
# forward engine
# ---------------------------------------------------------------------------

def resolve_events(inst):
    """Split an instruction's memory events into concrete byte-key sets."""
    read_keys = set()
    write_keys = set()
    read_list = []
    write_list = []
    for ev in inst.events:
        access = ev.arg2
        ea, size = ev.arg0, max(int(ev.arg1), 1)
        if access == pbtrace.ACCESS_WRITE:
            write_list.append((ea, size))
            write_keys |= _mem_keys(ea, size)
        else:  # read / read2
            read_list.append((ea, size))
            read_keys |= _mem_keys(ea, size)
    return {"read_keys": read_keys, "write_keys": write_keys,
            "reads": read_list, "writes": write_list}


def _bind_mem_eas(insn, events):
    """Give each explicit mem operand its concrete EA from the events."""
    reads = list(events["reads"])
    writes = list(events["writes"])
    for op in insn.operands:
        op.ea = None
        if op.kind != "mem" or insn.mnemonic == "lea":
            continue
        acc = op.access or _infer_access(insn.mnemonic, len(insn.operands),
                                         insn.operands.index(op))
        pool = []
        if acc & CS_AC_READ:
            pool += reads
        if acc & CS_AC_WRITE:
            pool += writes
        pick = None
        for ea, size in pool:
            if size == op.size:
                pick = ea
                break
        if pick is None and pool:
            pick = pool[0][0]
        op.ea = pick


def run_forward(window, decoder, sources, extra_sinks, max_sink_lines=50):
    regstate = RegState(decoder.arch)
    shadow = {}                       # ea byte -> taint entry
    sink_hits = []
    unknown_mnemonics = {}
    event_sourced = 0

    mem_sources = [s for s in sources if s.kind == "mem"]

    # seed @start sources
    for s in sources:
        lab = (frozenset([s.label]), 1)
        if s.kind == "reg":
            regstate.write(s.reg, lab)
        elif s.kind == "mem" and s.when == "start":
            for b in range(s.lo, s.hi):
                shadow[b] = lab

    def source_label_at(byte):
        for s in mem_sources:
            if s.when == "first-touch" and s.lo <= byte < s.hi:
                return s.label
        return None

    sink_ranges = [s for s in extra_sinks if s[0] == "mem"]

    for inst in window.insns:
        insn = decoder.decode(inst.ip, inst.rec)
        inst.decoded = insn
        events = resolve_events(inst)
        # attach concrete EAs to capstone's explicit mem operands
        for op in insn.operands:
            if op.kind == "mem":
                op.ea = None
        _bind_mem_eas(insn, events)

        # event:#N sources fire on the Nth memory event of the window
        fired_labels = set()
        for s in sources:
            if s.kind == "event" and s.event_index in inst.mem_indices:
                fired_labels.add(s.label)

        # first-touch: virgin shadow bytes inside a source range get labeled
        read_entries = dict()   # byte -> entry for this instruction's reads
        for ea, size in events["reads"]:
            for b in range(ea, ea + size):
                ent = shadow.get(b)
                if ent is None:
                    lab = source_label_at(b)
                    ent = (frozenset([lab]), 1) if lab else CLEAN
                read_entries[b] = ent
        if fired_labels:
            for ea, size in events["reads"]:
                for b in range(ea, ea + size):
                    prev = read_entries.get(b, CLEAN)
                    read_entries[b] = taint_union(
                        [prev, (frozenset(fired_labels), 1)])
            event_sourced += 1

        def read_taint(keys):
            out = []
            for k in keys:
                if k[0] == "m":
                    out.append(read_entries.get(k[1], shadow.get(k[1], CLEAN)))
                else:
                    bank = regstate.regs.get(k[1])
                    out.append(bank[k[2]] if bank else CLEAN)
            return taint_union(out)

        transfer = compute_transfer(insn, events, decoder.arch)
        if not transfer.known:
            unknown_mnemonics[insn.mnemonic] = \
                unknown_mnemonics.get(insn.mnemonic, 0) + 1

        # --- sinks -----------------------------------------------------
        cf_taint = read_taint(transfer.cf_uses)
        if cf_taint[0]:
            # built-in control-flow sink (a); --sink reg:RIP selects the same
            sink_hits.append((inst.seq, inst.ip, insn.text,
                              "control-flow", cf_taint))

        # apply steps, collecting per-write-event taint for the data sink
        written_entries = {}    # byte -> entry deposited this instruction
        for step in transfer.steps:
            src = read_taint(step.uses)
            if src[0]:
                src = (src[0], src[1] + 1)
            for k in step.def_full:
                if k in step.def_val:
                    ent = src
                else:
                    ent = CLEAN       # zero-extended upper bytes
                if k[0] == "m":
                    shadow[k[1]] = ent
                    written_entries[k[1]] = ent
                else:
                    regstate._bank(k[1])[k[2]] = ent

        if fired_labels and not events["reads"]:
            for ea, size in events["writes"]:
                for b in range(ea, ea + size):
                    shadow[b] = taint_union(
                        [shadow.get(b, CLEAN), (frozenset(fired_labels), 1)])
                    written_entries[b] = shadow[b]

        for ea, size in events["writes"]:
            ent = taint_union([written_entries.get(b, CLEAN)
                               for b in range(ea, ea + size)])
            if not ent[0]:
                continue
            in_source = any(s.lo <= ea and ea + size <= s.hi
                            for s in mem_sources)
            custom = any(lo <= ea and ea + size <= hi
                         for _k, lo, hi in sink_ranges)
            if custom:
                sink_hits.append((inst.seq, inst.ip, insn.text,
                                  "custom-mem-write", ent))
            elif not in_source:
                sink_hits.append((inst.seq, inst.ip, insn.text,
                                  "data-write", ent))

    stats = {
        "unknown_mnemonics": unknown_mnemonics,
        "undecoded": decoder.undecoded,
        "unpaired_mem": window.unpaired_mem,
        "insns": len(window.insns),
        "event_sourced": event_sourced,
    }
    state = {"regs": regstate, "shadow": shadow}
    return sink_hits, stats, state


# ---------------------------------------------------------------------------
# backward slice
# ---------------------------------------------------------------------------

def parse_operand(spec, arch=None):
    arch = arch or DEFAULT_ARCH
    if spec.startswith("reg:"):
        name = spec[4:].lower()
        if name not in arch.reg_info:
            raise SystemExit("unknown %s operand register: %s" % (arch.name, name))
        return ("reg", name)
    if spec.startswith("mem:"):
        parts = spec[4:].split(":")
        if len(parts) == 1:
            return ("mem", int(parts[0], 0), arch.pointer_size)
        return ("mem", int(parts[0], 0), int(parts[1], 0))
    raise SystemExit("operand syntax: reg:NAME | mem:0xEA[:0xSIZE]")


def run_slice(window, decoder, at_seq, operand):
    target = None
    for inst in window.insns:
        if inst.seq == at_seq:
            target = inst
    if target is None:
        # allow slicing at a memory event's owning instruction too
        for inst in window.insns:
            if any(ev.sequence == at_seq for ev in inst.events):
                target = inst
                break
    if target is None:
        raise SystemExit("seq %d not found in thread %d window"
                         % (at_seq, window.tid))

    for inst in window.insns[:target.idx + 1]:
        insn = decoder.decode(inst.ip, inst.rec)
        inst.decoded = insn
        events = resolve_events(inst)
        for op in insn.operands:
            if op.kind == "mem":
                op.ea = None
        _bind_mem_eas(insn, events)
        inst.transfer = compute_transfer(insn, events, decoder.arch)

    kind, a1, *rest = operand
    if kind == "reg":
        base, off, size = decoder.arch.reg_info[a1]
        demand = {("r", base, off + i) for i in range(size)}
    else:
        size = rest[0] if rest else decoder.arch.pointer_size
        demand = {("m", a1 + i) for i in range(size)}

    in_slice = {target.idx}
    # if the target instruction itself defines the operand, resolve through it
    # first (covers slicing at the producing instruction, not just a use site)
    for step in target.transfer.steps:
        if step.def_full & demand:
            demand -= step.def_full
            demand |= step.uses
    i = target.idx - 1
    while i >= 0 and demand:
        tr = window.insns[i].transfer
        hit = False
        for step in tr.steps:
            if step.def_full & demand:
                demand -= step.def_full
                demand |= step.uses
                hit = True
        # control-flow uses: the target instruction's input (e.g. jmp rax)
        # reads those bytes; nothing to resolve backwards beyond marking.
        if hit:
            in_slice.add(i)
        i -= 1

    # entry-boundary report
    reg_demand = {}
    mem_bytes = sorted(key[1] for key in demand if key[0] == "m")
    for key in demand:
        if key[0] == "r":
            reg_demand.setdefault(key[1], set()).add(key[2])
    return in_slice, reg_demand, mem_bytes, target


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _fmt_labels(entry):
    return "{%s}" % ",".join(sorted(entry[0])) if entry[0] else "{}"


def cmd_forward(args, trace):
    arch = architecture_from_meta(trace.meta)
    sources = [Source("s%d" % i, spec, arch)
               for i, spec in enumerate(args.source or [])]
    if not sources:
        raise SystemExit("forward needs at least one --source")
    extra_sinks = [parse_sink(s, arch) for s in (args.sink or [])]
    decoder = Decoder(args.pe, args.base, arch)
    window = build_window(trace, args.thread, args.max_events)
    sink_hits, stats, state = run_forward(window, decoder, sources, extra_sinks)
    print("== forward taint ==")
    print("window: thread=%d insns=%d records_consumed=%d"
          % (window.tid, stats["insns"], window.records_consumed))
    print("trace stats: %s" % trace.stats)
    if trace.stats.gap_records or trace.stats.truncated_tail:
        if trace.meta.get("post_filtered"):
            print("note: sequence gaps come from post-filtering "
                  "(meta.post_filtered); ring_missed=%s"
                  % trace.meta.get("ring_missed", "?"))
        else:
            print("WARNING: trace has holes — replay on a lossy window "
                  "is invalid!")
    print("sources:")
    for s in sources:
        print("  %s" % s.describe())
    print("sink hits: %d%s" % (len(sink_hits),
                                 " (showing first %d)" % args.max_sink_lines
                                 if len(sink_hits) > args.max_sink_lines else ""))
    for seq, ip, text, kind, ent in sink_hits[:args.max_sink_lines]:
        print("  [seq %-8d] 0x%012x  %-32s %-16s labels=%s chain=%d"
              % (seq, ip, text, kind, _fmt_labels(ent), ent[1]))
    if stats["unknown_mnemonics"]:
        detail = ", ".join("%s:%d" % kv
                           for kv in sorted(stats["unknown_mnemonics"].items()))
        print("unknown mnemonics (conservative union): %s" % detail)
    if stats["undecoded"]:
        print("undecoded instructions (no bytes): %d — provide kind-9 records "
              "or --pe" % stats["undecoded"])
    if stats["unpaired_mem"]:
        print("unpaired memory events: %d" % stats["unpaired_mem"])
    return 0


def cmd_slice(args, trace):
    if args.at is None or not args.operand:
        raise SystemExit("slice needs --at SEQ and --operand reg:NAME|mem:EA[:SZ]")
    arch = architecture_from_meta(trace.meta)
    decoder = Decoder(args.pe, args.base, arch)
    window = build_window(trace, args.thread, args.max_events)
    operand = parse_operand(args.operand, arch)
    in_slice, reg_demand, mem_demand, target = \
        run_slice(window, decoder, args.at, operand)
    print("== backward slice ==")
    print("window: thread=%d insns=%d | target seq=%d operand=%s"
          % (window.tid, len(window.insns), args.at, args.operand))
    print("slice: %d of %d instructions up to target"
          % (len(in_slice), target.idx + 1))
    for inst in window.insns[:target.idx + 1]:
        mark = "*" if inst.idx in in_slice else " "
        insn = inst.decoded
        print("  %s %-6d [seq %-8d] 0x%012x  %s"
              % (mark, inst.idx, inst.seq, inst.ip,
                 insn.text if insn else "??"))
    print("unresolved demand at window start:")
    if not reg_demand and not mem_demand:
        print("  (none — slice fully resolved inside the window)")
    for base, bytes_ in sorted(reg_demand.items()):
        print("  source outside window: %s@entry (bytes %s)"
              % (base.upper(), sorted(bytes_)))
    if mem_demand:
        # coalesce consecutive bytes into ranges
        start = prev = mem_demand[0]
        for b in mem_demand[1:]:
            if b == prev + 1:
                prev = b
                continue
            print("  source outside window: [0x%x-0x%x]@entry" % (start, prev + 1))
            start = prev = b
        print("  source outside window: [0x%x-0x%x]@entry" % (start, prev + 1))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("trace")
    sub = ap.add_subparsers(dest="cmd", required=True)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--thread", type=int, default=None,
                        help="thread_id (default: thread with most execs)")
    common.add_argument("--max-events", type=int, default=None)
    common.add_argument("--pe", default=None,
                        help="on-disk PE for instruction bytes (kind-3 traces)")
    common.add_argument("--base", type=lambda s: int(s, 0), default=None,
                        help="runtime base of --pe module (default: ImageBase)")

    fw = sub.add_parser("forward", parents=[common])
    fw.add_argument("--source", action="append",
                    help="reg:RAX | mem:0xA:0xSZ[@start|@first-touch] | event:#N")
    fw.add_argument("--sink", action="append",
                    help="extra sink: reg:RIP | mem:0xLO-0xHI")
    fw.add_argument("--max-sink-lines", type=int, default=50)

    sl = sub.add_parser("slice", parents=[common])
    sl.add_argument("--at", type=lambda s: int(s, 0))
    sl.add_argument("--operand", default=None)

    args = ap.parse_args(argv[1:])
    trace = pbtrace.load(args.trace)
    if args.pe and args.base is None:
        main_mod = trace.meta.get("main_module") or {}
        if main_mod.get("low"):
            args.base = main_mod["low"]   # recorded main-module load base
    if args.cmd == "forward":
        return cmd_forward(args, trace)
    return cmd_slice(args, trace)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
