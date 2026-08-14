# examples/python/replay — offline taint replay prototype (PinBridge)

Architecture (see `docs/taint-roadmap.md`): native engines **record** a window
at full speed; analysis — forward taint propagation and backward slicing —
runs **offline** in pure Python on the recording. Concrete EAs in the memory
events eliminate aliasing: no pointer analysis, no guessing.

## Files

- `pbtrace.py`  — `.pbtr` reader (header/meta/records, per-thread filter,
  stats: per-kind counts, sequence gaps, truncated-tail tolerance) plus the
  tiny writer used by the recorder and the unit tests. Pure stdlib.
- `recorder.py` — live recorder: speaks the agent's binary query protocol
  directly over loopback TCP (stdlib socket+struct; NOT pinbridge-cli, whose
  JSON truncates args). Produces contract-conformant `.pbtr`.
- `taint.py`    — the replay engine (pure Python + capstone): forward taint
  and backward slicing over single-thread windows.
- `test_taint.py` — offline unit tests (stdlib unittest, no Pin needed).

## .pbtr format contract (v1)

```
0:  "PBTR" (4 bytes)
4:  u32 version = 1
8:  u32 meta_len
12: u32 reserved = 0
16: meta_len bytes UTF-8 JSON: {"target": str, "created": str, "kinds": [u32], ...}
then fixed 88-byte records (all LE):
  +0 u64 sequence   +8 u32 kind   +12 u32 thread_id   +16 u64 address   +24..+80 u64 arg0..arg7
kind 3  exec:        address=ip, arg0=static instruction length
kind 2  memory:      address=ip, arg0=ea, arg1=size, arg2=access (0=read,1=write,2=read2)
kind 4  branch_edge: address=ip, arg0=target, arg1=taken
kind 9  exec_bytes:  address=ip, arg0=static_len, arg1=bytes[0..8), arg2=bytes[8..15) zero-padded
kind 10 mem_value:   address=ip, arg0=ea, arg1=size, arg2=access, arg3=value LE zero-padded
kind 11 marker:      address=0, arg0=tag, arg1=value
```

Readers skip unknown kinds and tolerate a truncated tail record. Records of
multiple threads interleave; split by `thread_id` for single-thread windows.
Recorder extras in meta: `main_module` {name, low, high}, `ring_missed`,
`post_filtered` (true when `--main-module-only` dropped records — sequence
gaps in such files are filtering artifacts, not ring loss).

## Recording

Launch the agent (engine range is **env-only** today):

```
set PINBRIDGE_AGENT_PORT=9011
set PINBRIDGE_AGENT_RANGE=0xLOW-0xHIGH   :: scope instrumentation; keeps the
:: event rate under the pull rate (~800K ev/s observed over loopback). Without
:: a range everything is instrumented — skip the startup flood by letting the
:: recorder start at the live ring edge (default) and record a calm window.
pin.exe -t pinbridge_agent.dll -- target.exe
```

Then:

```
python recorder.py --port 9011 --kinds exec,memory,branch --out win.pbtr \
    --seconds 3 [--main-module-only] [--target-name NAME]
```

The recorder arms engines via ENGINE_SET (2=memory 3=exec 4=branch), pulls
RING_PAGE in a tight loop, disarms, drains, writes header+meta+records, and
reports recorded/missed counts. **missed>0 or sequence holes = lossy window —
re-record narrower; replay refuses to trust holes.**

## Replaying

```
python taint.py win.pbtr forward \
    --source mem:0x7ff6e6e53128:4 [--source reg:RAX] [--source event:#0] \
    [--sink mem:0xLO-0xHI] [--sink reg:RIP] \
    [--thread N] [--max-events 2000000] \
    [--pe module.exe [--base 0x..]]

python taint.py win.pbtr slice --at 2711833 --operand mem:0x7ff6e6e53128:4 \
    [--pe module.exe]
```

- Instruction bytes come from kind-9 records when present; otherwise `--pe`
  maps bytes from the on-disk PE (default base: meta `main_module.low`, else
  the PE's ImageBase). **SMC caveat: the PE fallback breaks on self-modifying
  / self-decrypting code — for packed targets record 档2 (kind-9/10).**
- Sources: `reg:NAME` (seed at window entry), `mem:0xA:0xSZ[@first-touch|@start]`
  (first-touch labels virgin bytes when first read in the window),
  `event:#N` (labels the Nth memory event of the window).
- Sinks (built-in): (a) control-flow — taint reaches a jmp/call/ret target;
  (b) data — tainted memory write to an EA outside every source range.
  Extra `--sink mem:LO-HI` / `reg:RIP` on top.
- Slice: demand-set walk backwards from `--at` over `--operand`; memory demand
  resolves through concrete write EAs (exact, no alias analysis); unresolved
  demand at window start is reported as `source outside window: ...`.

## Unit tests

```
python test_taint.py     # 16 tests, pure offline, no Pin
```

Cover: reader round-trip/gaps/truncated tail/unknown kinds; mem→reg
propagation; xor-reg,reg kill; ALU label union; push/pop round-trip through
concrete stack EAs; 32-bit subregister zero-extend kill; control-flow & data
sink firing; conservative unknown-mnemonic handling; slice contributor
exactness, entry-boundary demand, stack propagation, xor-kill cut.

## Current limitations (v1)

- **档1 recordings are value-blind**: no data values in the events, so taint
  follows instruction semantics only (`and eax, 0` still propagates). kind-10
  values are carried but not yet used for constant folding.
- Register taint is byte-granular (8 slots per GP register); flags and
  SIMD/x87 registers are not tracked.
- Single-thread windows only (records are split by thread_id; inter-thread
  flows e.g. via shared memory are not followed).
- String/REP instructions (movs/stos/...) and other mnemonics outside the
  common-op subset take the conservative path (dest = union of sources) and
  are counted in the report.
- Multi-memory-operand instructions bind events to operands by access+size
  heuristics; exotic forms can mis-bind (counted, not silent).
- No concolic exploration: replay follows only the recorded path.
- The decode cache keys instruction bytes by ip — an SMC site that rewrites
  itself mid-window would be mis-decoded (kind-9/10 recordings don't have
  this problem once 档2 ships).
