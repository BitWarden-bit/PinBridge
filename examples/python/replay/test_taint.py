#!/usr/bin/env python3
"""test_taint.py — OFFLINE unit tests for the replay prototype.

No Pin, no live target: tests synthesize tiny .pbtr files with pbtrace.py's
writer and hand-assembled x86-64 instruction bytes carried as kind-9
exec_bytes / kind-10 mem_value records.

Run:  python test_taint.py        (stdlib unittest)
"""

from __future__ import annotations

import os
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import pbtrace
import taint

TID = 7
BASE_IP = 0x140001000

# hand-assembled x86-64
B_MOV_RAX_RBXP = b"\x48\x8B\x03"              # mov rax, [rbx]
B_MOV_RBXP_RAX = b"\x48\x89\x03"              # mov [rbx], rax
B_MOV_RCX_RAX = b"\x48\x89\xC1"               # mov rcx, rax
B_MOV_RAX_RDX = b"\x48\x89\xD0"               # mov rax, rdx
B_XOR_RAX_RAX = b"\x48\x31\xC0"               # xor rax, rax
B_ADD_RAX_RCX = b"\x48\x01\xC8"               # add rax, rcx
B_MOV_EAX_ECX = b"\x89\xC8"                   # mov eax, ecx (zero-extends!)
B_PUSH_RAX = b"\x50"                          # push rax
B_POP_RCX = b"\x59"                           # pop rcx
B_JMP_RAX = b"\xFF\xE0"                       # jmp rax
B_MOV_RCX_IMM = b"\x48\xB9" + struct.pack("<Q", 0x1234)   # mov rcx, 0x1234
B_CPUID = b"\x0F\xA2"                         # cpuid
B_RET = b"\xC3"                               # ret

EA_SRC = 0x0000000000200000
EA_SRC2 = 0x0000000000201000
EA_STACK = 0x0000000000300000


def build_trace(path, insns, tid=TID, meta_extra=None):
    """insns: list of (code_bytes, [(ea, size, access, value), ...])."""
    meta = {"target": "unit-test", "created": "2026-08-14T00:00:00Z",
            "kinds": [pbtrace.KIND_EXEC_BYTES, pbtrace.KIND_MEM_VALUE]}
    if meta_extra:
        meta.update(meta_extra)
    seq = 1
    ip = BASE_IP
    with pbtrace.TraceWriter(path, meta) as w:
        for code, mems in insns:
            padded = code + b"\x00" * (16 - len(code))
            a1, a2 = struct.unpack("<QQ", padded[:16])
            w.emit(seq, pbtrace.KIND_EXEC_BYTES, tid, ip, len(code), a1, a2)
            seq += 1
            for ea, size, access, value in mems:
                w.emit(seq, pbtrace.KIND_MEM_VALUE, tid, ip, ea, size,
                       access, value)
                seq += 1
            ip += len(code)
    return pbtrace.load(path)


def forward(trace, source_specs):
    sources = [taint.Source("s%d" % i, s) for i, s in enumerate(source_specs)]
    window = taint.build_window(trace, None)
    decoder = taint.Decoder()
    return taint.run_forward(window, decoder, sources, [])


class TraceReaderTests(unittest.TestCase):
    def test_roundtrip_stats_and_meta(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "t.pbtr")
            trace = build_trace(p, [(B_MOV_RAX_RBXP,
                                     [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)])])
            self.assertEqual(trace.meta["target"], "unit-test")
            self.assertEqual(trace.stats.counts[pbtrace.KIND_EXEC_BYTES], 1)
            self.assertEqual(trace.stats.counts[pbtrace.KIND_MEM_VALUE], 1)
            self.assertEqual(trace.stats.gap_records, 0)
            self.assertEqual(trace.dominant_thread(), TID)

    def test_truncated_tail_and_unknown_kind_and_gaps(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "t.pbtr")
            with pbtrace.TraceWriter(p, {"target": "x", "kinds": []}) as w:
                w.emit(1, pbtrace.KIND_EXEC, TID, BASE_IP, 5)
                w.emit(2, 77, TID, BASE_IP, 0)          # unknown kind: skipped
                w.emit(5, pbtrace.KIND_EXEC, TID, BASE_IP + 5, 5)  # gap 3..4
            with open(p, "ab") as fh:
                fh.write(b"\xde\xad\xbe\xef\x01")     # torn tail record
            trace = pbtrace.load(p)
            self.assertEqual(trace.stats.truncated_tail, 5)
            self.assertEqual(trace.stats.unknown_kinds, 1)
            self.assertEqual(trace.stats.gap_records, 2)
            self.assertEqual(len(trace.records), 2)   # unknown kind skipped


class ForwardTaintTests(unittest.TestCase):
    def test_a_mov_mem_to_reg_propagates(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"),
                                [(B_MOV_RAX_RBXP,
                                  [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)])])
            _hits, _stats, state = forward(trace, ["mem:0x%x:8" % EA_SRC])
            ent = state["regs"].read("rax")
            self.assertEqual(ent[0], frozenset({"s0"}))

    def test_b_xor_reg_reg_kills_taint(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_XOR_RAX_RAX, []),
            ])
            _hits, _stats, state = forward(trace, ["mem:0x%x:8" % EA_SRC])
            self.assertEqual(state["regs"].read("rax"), taint.CLEAN)

    def test_c_alu_unions_labels(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_MOV_RCX_RAX, []),
                (B_MOV_RAX_RBXP, [(EA_SRC2, 8, pbtrace.ACCESS_READ, 0)]),
                (B_ADD_RAX_RCX, []),
            ])
            hits, _stats, state = forward(
                trace, ["mem:0x%x:8" % EA_SRC, "mem:0x%x:8" % EA_SRC2])
            ent = state["regs"].read("rax")
            self.assertEqual(ent[0], frozenset({"s0", "s1"}))
            self.assertGreaterEqual(ent[1], 2)        # chain depth grows

    def test_d_push_pop_roundtrip_through_stack_ea(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_PUSH_RAX, [(EA_STACK, 8, pbtrace.ACCESS_WRITE, 0)]),
                (B_POP_RCX, [(EA_STACK, 8, pbtrace.ACCESS_READ, 0)]),
            ])
            _hits, _stats, state = forward(trace, ["mem:0x%x:8" % EA_SRC])
            self.assertEqual(state["regs"].read("rcx")[0], frozenset({"s0"}))

    def test_f_32bit_write_zero_extends_and_kills_upper_taint(self):
        with tempfile.TemporaryDirectory() as d:
            # rax fully tainted at entry, rcx tainted from another source;
            # mov eax, ecx must move rcx's taint into bytes 0..3 AND wipe 4..7
            trace = build_trace(os.path.join(d, "t.pbtr"), [(B_MOV_EAX_ECX, [])])
            _hits, _stats, state = forward(trace, ["reg:rax", "reg:rcx"])
            bank = state["regs"].regs["rax"]
            for i in range(4):
                self.assertEqual(bank[i][0], frozenset({"s1"}))
            for i in range(4, 8):
                self.assertEqual(bank[i], taint.CLEAN)

    def test_control_flow_and_data_sinks_fire(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_MOV_RBXP_RAX, [(EA_SRC2, 8, pbtrace.ACCESS_WRITE, 0)]),
                (B_JMP_RAX, []),
            ])
            hits, _stats, _state = forward(trace, ["mem:0x%x:8" % EA_SRC])
            kinds = [h[3] for h in hits]
            self.assertIn("data-write", kinds)        # write outside sources
            self.assertIn("control-flow", kinds)      # tainted jmp target
            cf = [h for h in hits if h[3] == "control-flow"][0]
            self.assertIn("jmp", cf[2])
            self.assertEqual(cf[4][0], frozenset({"s0"}))

    def test_write_inside_source_range_is_not_a_data_sink(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_MOV_RBXP_RAX, [(EA_SRC, 8, pbtrace.ACCESS_WRITE, 0)]),
            ])
            hits, _stats, _state = forward(trace, ["mem:0x%x:8" % EA_SRC])
            self.assertEqual([h for h in hits if h[3] == "data-write"], [])

    def test_unknown_mnemonic_counted_and_conservative(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [(B_CPUID, [])])
            _hits, stats, _state = forward(trace, ["reg:rbx"])
            self.assertEqual(stats["unknown_mnemonics"].get("cpuid"), 1)

    def test_event_source(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"),
                                [(B_MOV_RAX_RBXP,
                                  [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)])])
            _hits, _stats, state = forward(trace, ["event:#0"])
            self.assertEqual(state["regs"].read("rax")[0], frozenset({"s0"}))

    def test_disasm_sanity(self):
        # guard the hand-assembled bytes against typos
        decoder = taint.Decoder()
        ip = BASE_IP
        for code, want in ((B_MOV_RAX_RBXP, "mov rax, qword ptr [rbx]"),
                           (B_XOR_RAX_RAX, "xor rax, rax"),
                           (B_PUSH_RAX, "push rax"),
                           (B_JMP_RAX, "jmp rax"),
                           (B_MOV_EAX_ECX, "mov eax, ecx")):
            padded = code + b"\x00" * (16 - len(code))
            a1, a2 = struct.unpack("<QQ", padded[:16])
            rec = pbtrace.Record(1, pbtrace.KIND_EXEC_BYTES, TID, ip,
                                 (len(code), a1, a2, 0, 0, 0, 0, 0))
            insn = decoder.decode(ip, rec)
            self.assertEqual(insn.text, want)
            ip += 0x10


class BackwardSliceTests(unittest.TestCase):
    def test_e_slice_marks_only_contributors(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),  # idx 0
                (B_MOV_RCX_IMM, []),                                      # idx 1
                (B_JMP_RAX, []),                                          # idx 2
            ])
            window = taint.build_window(trace, None)
            jmp_seq = window.insns[2].seq
            in_slice, reg_demand, mem_demand, _target = taint.run_slice(
                window, taint.Decoder(), jmp_seq, ("reg", "rax"))
            self.assertEqual(in_slice, {0, 2})     # mov rcx,imm NOT in slice
            self.assertEqual(reg_demand, {})
            # the [rbx] read has no producer in-window -> entry-boundary demand
            self.assertEqual(mem_demand, list(range(EA_SRC, EA_SRC + 8)))

    def test_e2_slice_reg_demand_at_entry(self):
        with tempfile.TemporaryDirectory() as d:
            # mov rax, rdx: rax's producer needs rdx, which is never written
            trace = build_trace(os.path.join(d, "t.pbtr"), [(B_MOV_RAX_RDX, [])])
            window = taint.build_window(trace, None)
            in_slice, reg_demand, mem_demand, _t = taint.run_slice(
                window, taint.Decoder(), window.insns[0].seq, ("reg", "rax"))
            self.assertEqual(in_slice, {0})
            self.assertEqual(reg_demand, {"rdx": set(range(8))})
            self.assertEqual(mem_demand, [])

    def test_slice_through_stack(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_PUSH_RAX, [(EA_STACK, 8, pbtrace.ACCESS_WRITE, 0)]),
                (B_POP_RCX, [(EA_STACK, 8, pbtrace.ACCESS_READ, 0)]),
            ])
            window = taint.build_window(trace, None)
            in_slice, reg_demand, mem_demand, _t = taint.run_slice(
                window, taint.Decoder(), window.insns[2].seq, ("reg", "rcx"))
            self.assertEqual(in_slice, {0, 1, 2})
            self.assertEqual(reg_demand, {})
            self.assertEqual(mem_demand, list(range(EA_SRC, EA_SRC + 8)))

    def test_slice_stops_at_xor_kill(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_XOR_RAX_RAX, []),
                (B_MOV_RCX_RAX, []),
            ])
            window = taint.build_window(trace, None)
            in_slice, reg_demand, mem_demand, _t = taint.run_slice(
                window, taint.Decoder(), window.insns[2].seq, ("reg", "rcx"))
            # xor kills rax: the earlier mem->rax load must NOT be in the slice
            self.assertEqual(in_slice, {1, 2})
            self.assertEqual(reg_demand, {})
            self.assertEqual(mem_demand, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
