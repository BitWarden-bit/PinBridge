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
B_XCHG_RAX_RCX = b"\x48\x91"                  # xchg rax, rcx
B_PUSH_RAX = b"\x50"                          # push rax
B_POP_RCX = b"\x59"                           # pop rcx
B_JMP_RAX = b"\xFF\xE0"                       # jmp rax
B_MOV_RCX_IMM = b"\x48\xB9" + struct.pack("<Q", 0x1234)   # mov rcx, 0x1234
B_CPUID = b"\x0F\xA2"                         # cpuid
B_RET = b"\xC3"                               # ret
B_X86_INC_EAX_RET = b"\x40\xC3"               # x86: inc eax; ret
B_X86_MOV_EAX_MEM_EBX = b"\x8B\x03"            # x86: mov eax, [ebx]

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
    def test_scope_add_marker_projects_into_metadata(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "scope.pbtr")
            with pbtrace.TraceWriter(p, {"target": "scope", "kinds": []}) as w:
                w.emit(1, pbtrace.KIND_MARKER, 0, 0,
                       pbtrace.MARKER_SCOPE_ADD, 0x500000, 0x502000)
            trace = pbtrace.load(p)
            self.assertEqual(trace.meta["scope_additions"],
                             [[0x500000, 0x502000]])

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

    def test_repeat_marker_expands_without_sequence_gaps(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "repeat.pbtr")
            with pbtrace.TraceWriter(p, {"target": "repeat", "kinds": [pbtrace.KIND_EXEC]}) as w:
                w.emit(1, pbtrace.KIND_EXEC, TID, BASE_IP, 3)
                w.emit(4, pbtrace.KIND_REPEAT, TID, BASE_IP, 3,
                       pbtrace.KIND_EXEC)
                w.emit(5, pbtrace.KIND_EXEC, TID, BASE_IP + 3, 2)

            trace = pbtrace.load(p)
            self.assertEqual([r.sequence for r in trace.records], [1, 2, 3, 4, 5])
            self.assertEqual([r.kind for r in trace.records],
                             [pbtrace.KIND_EXEC] * 5)
            self.assertEqual(trace.stats.physical_records, 3)
            self.assertEqual(trace.stats.logical_records, 5)
            self.assertEqual(trace.stats.repeat_markers, 1)
            self.assertEqual(trace.stats.repeated_records, 3)
            self.assertEqual(trace.stats.gap_records, 0)
            self.assertAlmostEqual(trace.stats.compression_ratio, 5.0 / 3.0)

            compact = pbtrace.load(p, expand_repeats=False)
            self.assertEqual([r.kind for r in compact.records],
                             [pbtrace.KIND_EXEC, pbtrace.KIND_REPEAT,
                              pbtrace.KIND_EXEC])

    def test_repeat_marker_must_match_previous_payload(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "bad-repeat.pbtr")
            with pbtrace.TraceWriter(p, {"target": "repeat", "kinds": []}) as w:
                w.emit(1, pbtrace.KIND_EXEC, TID, BASE_IP, 3)
                # sequence and payload deliberately disagree with the base
                w.emit(9, pbtrace.KIND_REPEAT, TID, BASE_IP, 3,
                       pbtrace.KIND_MEMORY)
            trace = pbtrace.load(p)
            self.assertEqual(trace.stats.invalid_repeats, 1)
            self.assertEqual(trace.stats.logical_records, 2)
            self.assertEqual(len(trace.records), 2)

    def test_typed_json_projection(self):
        rec = pbtrace.Record(4, pbtrace.KIND_MEM_VALUE, TID, BASE_IP,
                             (EA_SRC, 4, pbtrace.ACCESS_READ, 0x1234, 0, 0, 0, 0))
        view = rec.as_dict()
        self.assertEqual(view["kind"], "mem_value")
        self.assertEqual(view["ip"], "0x%x" % BASE_IP)
        self.assertEqual(view["memory"], "0x%x" % EA_SRC)
        self.assertEqual(view["value"], "0x1234")

    def test_trace_writer_rle_roundtrip(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "writer-rle.pbtr")
            with pbtrace.TraceWriter(p, {"target": "rle"},
                                     compress_repeats=True) as w:
                for seq in range(1, 6):
                    w.emit(seq, pbtrace.KIND_EXEC, TID, BASE_IP, 1)
                self.assertEqual(w.count, 5)
            self.assertEqual(w.physical_count, 2)
            trace = pbtrace.load(p)
            self.assertEqual(len(trace.records), 5)
            self.assertEqual(trace.stats.repeated_records, 4)

    def test_register_snapshot_projection(self):
        rec = pbtrace.Record(8, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                             (10, 0x1234, 0, 8, 0, 0, 0, 9))
        view = rec.as_dict()
        self.assertEqual(view["reg"], "rax")
        self.assertEqual(view["value"], "0x0000000000001234")
        self.assertEqual(view["frame"], 9)

    def test_trace_frames_restore_register_delta_and_attach_events(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "frames.pbtr")
            code = B_XOR_RAX_RAX
            a1, a2 = struct.unpack("<QQ", (code + b"\x00" * 13)[:16])
            with pbtrace.TraceWriter(p, {"target": "frames"}) as w:
                mask = (1 << 0) | (1 << 17)  # rax + rflags
                w.emit(1, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                       0, mask, 0, 1, 0, 0, 0, 1)
                w.emit(2, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                       10, 0x1111, 0, 8, 0, 0, 0, 1)
                w.emit(3, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                       25, 0x202, 0, 8, 0, 0, 0, 1)
                w.emit(4, pbtrace.KIND_EXEC_BYTES, TID, BASE_IP,
                       len(code), a1, a2)
                w.emit(5, pbtrace.KIND_MEM_VALUE, TID, BASE_IP,
                       EA_SRC, 8, pbtrace.ACCESS_READ, 0x55)
                w.emit(6, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP + 3,
                       0, 1, 0, 2, 0, 0, 0, 2)
                w.emit(7, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP + 3,
                       10, 0x2222, 0, 8, 0, 0, 0, 2)
                w.emit(8, pbtrace.KIND_EXEC_BYTES, TID, BASE_IP + 3,
                       len(code), a1, a2)

            frames = pbtrace.load(p).frames()
            self.assertEqual(len(frames), 2)
            self.assertEqual(frames[0].registers["rax"], "0x1111")
            self.assertEqual(frames[0].registers["rflags"], "0x202")
            self.assertEqual(frames[0].memory[0]["value"], "0x55")
            self.assertEqual(frames[1].registers["rax"], "0x2222")
            self.assertEqual(frames[1].registers["rflags"], "0x202")

    def test_exec_bytes_and_exec_share_one_instruction_frame(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "dual-exec.pbtr")
            code = B_XOR_RAX_RAX
            a1, a2 = struct.unpack("<QQ", (code + b"\x00" * 13)[:16])
            with pbtrace.TraceWriter(p, {"target": "dual", "arch": "x64"}) as w:
                w.emit(1, pbtrace.KIND_EXEC_BYTES, TID, BASE_IP,
                       len(code), a1, a2)
                w.emit(2, pbtrace.KIND_EXEC, TID, BASE_IP, len(code))
            frames = pbtrace.load(p).frames()
            self.assertEqual(len(frames), 1)
            self.assertEqual([r.kind for r in frames[0].records],
                             [pbtrace.KIND_EXEC_BYTES, pbtrace.KIND_EXEC])
            self.assertEqual(frames[0].machine_code, code)

    def test_incomplete_register_snapshot_does_not_advance_state(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "incomplete-reg.pbtr")
            with pbtrace.TraceWriter(p, {"target": "regs", "arch": "x64"}) as w:
                # Header promises two registers, but only RAX arrives.
                w.emit(1, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                       0, 0b11, 0, 1, 0, 0, 0, 1)
                w.emit(2, pbtrace.KIND_REG_SNAPSHOT, TID, BASE_IP,
                       10, 0x1111, 0, 8, 0, 0, 0, 1)
                w.emit(3, pbtrace.KIND_EXEC, TID, BASE_IP, 1)
            frames = pbtrace.load(p).frames()
            self.assertEqual(len(frames), 1)
            self.assertFalse(frames[0].context_complete)
            self.assertEqual(frames[0].registers, {})

    def test_x86_register_snapshot_uses_x86_name(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "x86-reg.pbtr")
            with pbtrace.TraceWriter(p, {"target": "x86", "arch": "x86"}) as w:
                w.emit(1, pbtrace.KIND_REG_SNAPSHOT, TID, 0x401000,
                       0, 1, 0, 1, 0, 0, 0, 1)
                w.emit(2, pbtrace.KIND_REG_SNAPSHOT, TID, 0x401000,
                       56, 0x1234, 0, 8, 0, 0, 0, 1)
                w.emit(3, pbtrace.KIND_EXEC, TID, 0x401000, 1)
            frame = pbtrace.load(p).frames()[0]
            self.assertEqual(frame.registers["eax"], "0x1234")

    def test_trace_frame_attaches_syscall_and_exception_events(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "os-events.pbtr")
            with pbtrace.TraceWriter(p, {"target": "events"}) as w:
                w.emit(1, pbtrace.KIND_SYSCALL, TID, BASE_IP,
                       0x55, 0, 1, 2, 3, 4, 5, 6)
                w.emit(2, pbtrace.KIND_EXEC, TID, BASE_IP, 2)
                w.emit(3, pbtrace.KIND_CONTEXT_CHANGE, TID, BASE_IP,
                       1, 0xC0000005, BASE_IP)
            frames = pbtrace.load(p).frames()
            self.assertEqual(len(frames), 1)
            self.assertEqual(frames[0].syscalls[0]["number"], 0x55)
            self.assertEqual(frames[0].syscalls[0]["args"],
                             ["0x1", "0x2", "0x3", "0x4", "0x5", "0x6"])
            self.assertEqual(frames[0].exceptions[0]["context_ip"],
                             "0x%x" % BASE_IP)


class ForwardTaintTests(unittest.TestCase):
    def test_x86_mode_and_register_model_follow_trace_metadata(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(
                os.path.join(d, "x86.pbtr"),
                [(B_X86_MOV_EAX_MEM_EBX,
                  [(EA_SRC, 4, pbtrace.ACCESS_READ, 0)])],
                meta_extra={"arch": "x86", "pointer_width": 4},
            )
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            sources = [taint.Source("s0", "mem:0x%x:4" % EA_SRC, arch)]
            _hits, _stats, state = taint.run_forward(
                taint.build_window(trace, None), decoder, sources, [])
            self.assertEqual(decoder.arch.name, "x86")
            self.assertEqual(state["regs"].read("eax")[0], frozenset({"s0"}))
            self.assertEqual(len(state["regs"].regs["eax"]), 4)
            with self.assertRaises(SystemExit):
                taint.Source("bad", "reg:rax", arch)

    def test_x86_0x40_is_inc_eax_not_a_rex_prefix(self):
        padded = B_X86_INC_EAX_RET + b"\x00" * (16 - len(B_X86_INC_EAX_RET))
        a1, a2 = struct.unpack("<QQ", padded)
        rec = pbtrace.Record(
            1, pbtrace.KIND_EXEC_BYTES, TID, 0x401000,
            (len(B_X86_INC_EAX_RET), a1, a2, 0, 0, 0, 0, 0),
        )
        insn = taint.Decoder(arch="x86").decode(0x401000, rec)
        self.assertEqual(insn.text, "inc eax")
        self.assertEqual(insn.size, 1)

    def test_decode_cache_distinguishes_smc_bytes_at_the_same_ip(self):
        decoder = taint.Decoder()

        def record(code, sequence):
            padded = code + b"\x00" * (16 - len(code))
            a1, a2 = struct.unpack("<QQ", padded)
            return pbtrace.Record(
                sequence, pbtrace.KIND_EXEC_BYTES, TID, BASE_IP,
                (len(code), a1, a2, 0, 0, 0, 0, 0),
            )

        self.assertEqual(decoder.decode(BASE_IP, record(b"\x90", 1)).text, "nop")
        self.assertEqual(decoder.decode(BASE_IP, record(B_RET, 2)).text, "ret")

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

    def test_xchg_reads_both_operands_before_writing_either(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_XCHG_RAX_RCX, []),
            ])
            _hits, _stats, state = forward(trace, ["reg:rax", "reg:rcx"])
            self.assertEqual(state["regs"].read("rax")[0],
                             frozenset({"s1"}))
            self.assertEqual(state["regs"].read("rcx")[0],
                             frozenset({"s0"}))

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

    def test_custom_register_sink_fires_on_tainted_alias_write(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_MOV_RAX_RBXP, [(EA_SRC, 8, pbtrace.ACCESS_READ, 0)]),
                (B_MOV_RCX_RAX, []),
            ])
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            hits, _stats, _state = taint.run_forward(
                taint.build_window(trace, None), decoder,
                [taint.Source("s0", "mem:0x%x:8" % EA_SRC, arch)],
                [taint.parse_sink("reg:ecx", arch)],
            )
            reg_hits = [h for h in hits if h[3] == "custom-reg-write"]
            self.assertEqual(len(reg_hits), 1)
            self.assertIn("mov rcx, rax", reg_hits[0][2])
            self.assertEqual(reg_hits[0][4][0], frozenset({"s0"}))

    def test_custom_register_sink_ignores_clean_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_XOR_RAX_RAX, []),
            ])
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            hits, _stats, state = taint.run_forward(
                taint.build_window(trace, None), decoder,
                [taint.Source("s0", "reg:rax", arch)],
                [taint.parse_sink("reg:rax", arch)],
            )
            self.assertEqual([h for h in hits
                              if h[3] == "custom-reg-write"], [])
            self.assertEqual(state["regs"].read("rax"), taint.CLEAN)

    def test_x86_custom_register_sink_uses_x86_aliases(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(
                os.path.join(d, "x86-sink.pbtr"),
                [(B_X86_MOV_EAX_MEM_EBX,
                  [(EA_SRC, 4, pbtrace.ACCESS_READ, 0)])],
                meta_extra={"arch": "x86", "pointer_width": 4},
            )
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            hits, _stats, _state = taint.run_forward(
                taint.build_window(trace, None), decoder,
                [taint.Source("s0", "mem:0x%x:4" % EA_SRC, arch)],
                [taint.parse_sink("reg:ax", arch)],
            )
            reg_hits = [h for h in hits if h[3] == "custom-reg-write"]
            self.assertEqual(len(reg_hits), 1)
            self.assertEqual(reg_hits[0][4][0], frozenset({"s0"}))

    def test_instruction_pointer_sink_reuses_control_flow_hit(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_JMP_RAX, []),
            ])
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            hits, _stats, _state = taint.run_forward(
                taint.build_window(trace, None), decoder,
                [taint.Source("s0", "reg:rax", arch)],
                [taint.parse_sink("reg:rip", arch)],
            )
            self.assertEqual([h[3] for h in hits], ["control-flow"])

    def test_sink_samples_are_bounded_while_total_stays_exact(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_JMP_RAX, []),
                (B_JMP_RAX, []),
                (B_JMP_RAX, []),
                (B_JMP_RAX, []),
            ])
            arch = taint.architecture_from_meta(trace.meta)
            decoder = taint.Decoder(arch=arch)
            hits, stats, _state = taint.run_forward(
                taint.build_window(trace, None), decoder,
                [taint.Source("s0", "reg:rax", arch)], [],
                max_sink_lines=2,
            )
            self.assertEqual(len(hits), 2)
            self.assertEqual(stats["sink_hits"], 4)

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
    def test_slice_through_xchg_uses_pre_instruction_operand(self):
        with tempfile.TemporaryDirectory() as d:
            trace = build_trace(os.path.join(d, "t.pbtr"), [
                (B_XCHG_RAX_RCX, []),
            ])
            window = taint.build_window(trace, None)
            in_slice, reg_demand, mem_demand, _target = taint.run_slice(
                window, taint.Decoder(), window.insns[0].seq, ("reg", "rcx"))
            self.assertEqual(in_slice, {0})
            self.assertEqual(reg_demand, {"rax": set(range(8))})
            self.assertEqual(mem_demand, [])

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
