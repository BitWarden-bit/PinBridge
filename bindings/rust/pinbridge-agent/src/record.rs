//! Trace recording channel (taint-roadmap layer 2): lossless capture of one
//! address window into a dedicated slab, drained to a .pbtr file by a Pin
//! internal thread. Independent of the 64K main ring: record capture
//! callbacks submit ONLY here, the main-ring engines keep their own
//! insertions and enable semantics.
//!
//! Buffer: one preallocated slab (never freed once created — lock-free
//! producers can be preempted between the pointer load and the slot write,
//! so no reclamation point is provably safe; same trade as hooks.rs). Each
//! slot is 96 bytes in memory: an 88-byte in-memory Event plus its 8-byte
//! sequence tag (kept in a parallel array so a session reset is one memset).
//! Capacity comes from PINBRIDGE_AGENT_RECORD_CAP (default 1_000_000) and is
//! fixed at the FIRST trace_start; later sessions reuse the slab.
//!
//! Producer protocol (analysis callbacks, hot path — no allocation, no std
//! locks, no I/O): reserve with a bounded compare/exchange only while the
//! slab has capacity; a full slab increments DROPPED without consuming a
//! sequence number; after reservation the producer always writes the 88-byte
//! payload and publishes the tag (claim + 1) with Release ordering. Stop
//! waits for in-flight producers, so the drainer never guesses across holes.
//!
//! Drainer (Pin internal thread, std io allowed there): polls tags in
//! order, appends 88-byte wire-layout records (pinbridge-proto EventRecord)
//! to the file, finishes after trace_stop once caught up. Kind-11 marker
//! records bracket the tape (start/end annotations). Consecutive identical
//! events are written once followed by a kind-12 repeat marker; readers can
//! expand that marker to recover the original logical event stream.
//!
//! Arming: trace_start publishes the runtime range atomics, sets ARMED (and
//! the sticky instrumented flag), then flushes the JIT for [lo, hi) so the
//! window re-instruments. Insertions are gated on (ARMED || STICKY) &&
//! instrumentation-time range, and every record callback re-checks
//! ARMED + range + kind bit at analysis time, so stale insertions in
//! already-JITted code stay inert.
//!
//! File format (FIXED CONTRACT, see docs/taint-roadmap.md):
//!   0:  "PBTR" (4 bytes)
//!   4:  u32 version = 1
//!   8:  u32 meta_len
//!   12: u32 reserved = 0
//!   16: meta_len bytes UTF-8 JSON {"target","created","kinds","agent","note", ...}
//!   then 88-byte EventRecord wire images. Register snapshots, when requested,
//!   are emitted as kind-13 components from the instruction context callback;
//!   no separate entry_context blob is required.

use crate::event::{
    Event, EVENT_BRANCH_EDGE, EVENT_CONTEXT_CHANGE, EVENT_EXEC, EVENT_EXEC_BYTES,
    EVENT_MARKER, EVENT_MEMORY, EVENT_MEM_VALUE, EVENT_REG_SNAPSHOT, EVENT_REPEAT,
    EVENT_SYSCALL,
};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicPtr, AtomicUsize, Ordering};
use pinbridge_sys::*;
use std::io::Write;

/// Default slot capacity of the record slab (one session's tape).
const DEFAULT_CAP: usize = 1_000_000;
/// u64 words per slot payload (88-byte Event).
const SLOT_WORDS: usize = 11;
/// Marker tags (kind 11, arg0).
const MARKER_START: u64 = 1;
const MARKER_STOP: u64 = 2;
const MARKER_SCOPE_ADD: u64 = 3;
/// Kinds the record channel can capture: memory, exec, branch, values,
/// register snapshots, syscall and context-change events.
pub const RECORDABLE_MASK: u32 = (1 << EVENT_MEMORY)
    | (1 << EVENT_EXEC)
    | (1 << EVENT_BRANCH_EDGE)
    | (1 << EVENT_EXEC_BYTES)
    | (1 << EVENT_MEM_VALUE)
    | (1 << EVENT_REG_SNAPSHOT)
    | (1 << EVENT_SYSCALL)
    | (1 << EVENT_CONTEXT_CHANGE);

static CONTEXT_FRAME: AtomicU64 = AtomicU64::new(0);
const CONTEXT_SLOTS: usize = 64;
const NO_CONTEXT_OWNER: u32 = u32::MAX;
const CONTEXT_REG_COUNT: usize = 114; // 18 GP/flags + 32 XMM + 32 YMM + 32 ZMM
const CONTEXT_CHUNKS: usize = 8; // max 512-bit ZMM value

struct ContextSlot {
    owner: AtomicU32,
    valid: AtomicBool,
    values: [AtomicU64; CONTEXT_REG_COUNT * CONTEXT_CHUNKS],
}

impl ContextSlot {
    const fn new() -> Self {
        Self {
            owner: AtomicU32::new(NO_CONTEXT_OWNER),
            valid: AtomicBool::new(false),
            values: [const { AtomicU64::new(0) }; CONTEXT_REG_COUNT * CONTEXT_CHUNKS],
        }
    }
}

static CONTEXT_STATE: [ContextSlot; CONTEXT_SLOTS] =
    [const { ContextSlot::new() }; CONTEXT_SLOTS];

/// Register id for a wire slot. The front GP slots follow
/// `crate::arch::gp_registers()` so an ia32 build emits eax/.../eip/eflags
/// instead of rax/.../rip; the x64-only trailing GP slots (r8-r15/rip/rflags
/// positions) then report `PB_REG_INVALID_` and are never read, so no fake
/// values are produced. XMM/YMM/ZMM slots are arch-independent.
#[inline]
fn context_reg(index: usize) -> PbRegId {
    match index {
        0..=17 => {
            let gp = crate::arch::gp_registers();
            if index < gp.len() {
                gp[index].1
            } else {
                PB_REG_INVALID_
            }
        }
        18..=49 => PB_REG_XMM0 + (index - 18) as PbRegId,
        50..=81 => PB_REG_YMM0 + (index - 50) as PbRegId,
        82..=113 => PB_REG_ZMM0 + (index - 82) as PbRegId,
        _ => PB_REG_INVALID_,
    }
}

#[inline]
fn context_width(index: usize) -> usize {
    match index {
        // GP slots: 8 bytes on both arches (the agent reads every GP register
        // as a scalar u64, matching context.rs CONTEXT_GET). x86 has only 10
        // GP registers, so its trailing slots report 0 and are never read.
        0..=17 => {
            if index < crate::arch::gp_registers().len() {
                8
            } else {
                0
            }
        }
        18..=49 => 16,
        50..=81 => 32,
        82..=113 => 64,
        _ => 0,
    }
}

fn context_slot(thread_id: u32) -> Option<&'static ContextSlot> {
    for slot in &CONTEXT_STATE {
        let owner = slot.owner.load(Ordering::Acquire);
        if owner == thread_id {
            return Some(slot);
        }
        if owner == NO_CONTEXT_OWNER
            && slot
                .owner
                .compare_exchange(
                    NO_CONTEXT_OWNER,
                    thread_id,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
        {
            return Some(slot);
        }
    }
    None
}

fn reset_context_state() {
    for slot in &CONTEXT_STATE {
        slot.valid.store(false, Ordering::Release);
        slot.owner.store(NO_CONTEXT_OWNER, Ordering::Release);
    }
}

struct Slab {
    /// Per-slot sequence tag (plain u64 storage, accessed as AtomicU64 —
    /// same size/alignment/validity, so the cast is sound).
    tags: Box<[u64]>,
    /// cap * SLOT_WORDS words of payload storage (one Event per slot).
    payloads: Box<[u64]>,
}

static SLAB: AtomicPtr<Slab> = AtomicPtr::new(core::ptr::null_mut());
static SLAB_CAP: AtomicUsize = AtomicUsize::new(0);

/// Successfully reserved sequence slots this session (producers).
static CLAIM: AtomicU64 = AtomicU64::new(0);
/// Drainer cursor: slots consumed this session.
static DRAINED: AtomicU64 = AtomicU64::new(0);
/// Capacity/IO drops this session. A capacity drop never advances CLAIM.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// Logical capture events drained this session (before kind-12 RLE).
static WRITTEN: AtomicU64 = AtomicU64::new(0);
/// Recording live: producers may claim. Cleared by trace_stop.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Drainer finished (file closed, caught up). Set once per session.
static DRAIN_DONE: AtomicBool = AtomicBool::new(true);
/// Producers that entered submit(). Stop/drain waits for all pre-disarm
/// reservations to publish before declaring the sequence stream complete.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
/// Terminal health of the last/current session. Capacity overflow is reported
/// by DROPPED; this flag is reserved for an incomplete/corrupt output stream.
static FAILED: AtomicBool = AtomicBool::new(false);
static SESSION_STARTED: AtomicBool = AtomicBool::new(false);
pub const SESSION_IDLE: u8 = 0;
pub const SESSION_RECORDING: u8 = 1;
pub const SESSION_DRAINING: u8 = 2;
pub const SESSION_COMPLETE: u8 = 3;
pub const SESSION_FAILED: u8 = 4;
/// Sticky instrumentation flag: once a session armed, record insertions
/// keep landing in newly JITted code for the armed range (inert while
/// ARMED is down — the analysis-time re-check gates them).
static STICKY: AtomicBool = AtomicBool::new(false);
/// Runtime record window + kind mask (instrumentation- and analysis-time).
static RECORD_LO: AtomicU64 = AtomicU64::new(0);
static RECORD_HI: AtomicU64 = AtomicU64::new(0);
static RECORD_KINDS: AtomicU64 = AtomicU64::new(0);
const MAX_RECORD_RANGES: usize = 16;
const MAX_RECORD_THREADS: usize = 64;
static RECORD_RANGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static RECORD_RANGE_LO: [AtomicU64; MAX_RECORD_RANGES] =
    [const { AtomicU64::new(0) }; MAX_RECORD_RANGES];
static RECORD_RANGE_HI: [AtomicU64; MAX_RECORD_RANGES] =
    [const { AtomicU64::new(0) }; MAX_RECORD_RANGES];
static RECORD_THREAD_COUNT: AtomicUsize = AtomicUsize::new(0);
static RECORD_THREADS: [AtomicU32; MAX_RECORD_THREADS] =
    [const { AtomicU32::new(0) }; MAX_RECORD_THREADS];

// ---- producer side (analysis callbacks; hot-path discipline) ----

/// Analysis-time gate: recording live + address inside the window + this
/// kind requested. Already-JITted code with stale insertions dies here.
#[inline]
fn in_record_ranges(address: u64) -> bool {
    let count = RECORD_RANGE_COUNT.load(Ordering::Acquire);
    if count == 0 {
        return address >= RECORD_LO.load(Ordering::Relaxed)
            && address < RECORD_HI.load(Ordering::Relaxed);
    }
    for index in 0..count.min(MAX_RECORD_RANGES) {
        if address >= RECORD_RANGE_LO[index].load(Ordering::Relaxed)
            && address < RECORD_RANGE_HI[index].load(Ordering::Relaxed)
        {
            return true;
        }
    }
    false
}

#[inline]
fn thread_allowed(thread_id: u32) -> bool {
    let count = RECORD_THREAD_COUNT.load(Ordering::Relaxed);
    if count == 0 {
        return true;
    }
    for index in 0..count.min(MAX_RECORD_THREADS) {
        if RECORD_THREADS[index].load(Ordering::Relaxed) == thread_id {
            return true;
        }
    }
    false
}

#[inline]
fn armed_in_range(address: u64, thread_id: u32, kind: u32) -> bool {
    ARMED.load(Ordering::Acquire)
        && (RECORD_KINDS.load(Ordering::Relaxed) & (1 << kind) as u64) != 0
        && in_record_ranges(address)
        && thread_allowed(thread_id)
}

/// Submit a global syscall/context event to the trace channel. These events
/// are already registered by the engine and therefore need no instruction
/// re-JIT; the current context RIP supplies their range association.
pub unsafe fn submit_global(context: PbConstContextHandle, mut event: Event) {
    let mut rip = 0u64;
    if context.is_null()
        || pb_pin_get_context_reg(context, crate::arch::instr_ptr_reg(), &mut rip) != PB_OK
        || !armed_in_range(rip, event.thread_id, event.kind)
    {
        return;
    }
    event.address = rip;
    submit(event);
}

#[inline]
fn try_reserve(claim: &AtomicU64, drained: &AtomicU64, cap: u64) -> Option<u64> {
    if cap == 0 {
        return None;
    }
    let mut current = claim.load(Ordering::Relaxed);
    loop {
        if current.wrapping_sub(drained.load(Ordering::Acquire)) >= cap {
            return None;
        }
        match claim.compare_exchange_weak(
            current,
            current.wrapping_add(1),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(current),
            Err(observed) => current = observed,
        }
    }
}

/// Hot-path entry point: record one event into the slab. Never blocks. A full
/// slab drops without consuming a sequence number, so the drainer never has
/// to wait through artificial overflow holes.
#[inline]
pub fn submit(mut event: Event) -> bool {
    let slab = SLAB.load(Ordering::Acquire);
    if slab.is_null() || !ARMED.load(Ordering::Acquire) {
        return false;
    }
    IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
    let recorded = (|| {
        // Close the race with trace_stop after the optimistic entry check.
        if !ARMED.load(Ordering::Acquire) {
            return false;
        }
        let cap = SLAB_CAP.load(Ordering::Relaxed);
        let Some(claim) = try_reserve(&CLAIM, &DRAINED, cap as u64) else {
            // No CLAIM increment: capacity drops cannot create sequence holes.
            DROPPED.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        // Once reserved, always publish. trace_stop waits for IN_FLIGHT before
        // allowing the drainer to close the file.
        event.sequence = claim + 1;
        let slot = (claim % cap as u64) as usize;
        unsafe {
            let slab = &*slab;
            let dst = slab.payloads.as_ptr().add(slot * SLOT_WORDS) as *mut Event;
            // 8-aligned by construction; the consumer reads only after the tag
            // lands (Release/Acquire), so the payload write happens-first.
            core::ptr::write(dst, event);
            let tag = &*(slab.tags.as_ptr().add(slot) as *const AtomicU64);
            tag.store(claim + 1, Ordering::Release);
        }
        true
    })();
    IN_FLIGHT.fetch_sub(1, Ordering::Release);
    recorded
}

pub unsafe extern "C" fn on_rec_exec_bytes(
    address: u64,
    thread_id: u32,
    size: u32,
    bytes_lo: u64,
    bytes_hi: u64,
    _user_data: *mut c_void,
) {
    if !armed_in_range(address, thread_id, EVENT_EXEC_BYTES) {
        return;
    }
    submit(Event {
        kind: EVENT_EXEC_BYTES,
        thread_id,
        address,
        arg0: size as u64,
        arg1: bytes_lo,
        arg2: bytes_hi,
        ..Event::EMPTY
    });
}

pub unsafe extern "C" fn on_rec_mem_value(
    instruction_address: u64,
    thread_id: u32,
    memory_address: u64,
    size: u32,
    access: u32,
    value: u64,
    _user_data: *mut c_void,
) {
    if !armed_in_range(instruction_address, thread_id, EVENT_MEM_VALUE) {
        return;
    }
    submit(Event {
        kind: EVENT_MEM_VALUE,
        thread_id,
        address: instruction_address,
        arg0: memory_address,
        arg1: size as u64,
        arg2: access as u64,
        arg3: value,
        ..Event::EMPTY
    });
}

pub unsafe extern "C" fn on_rec_exec(
    address: u64,
    thread_id: u32,
    size: u32,
    _user_data: *mut c_void,
) {
    if !armed_in_range(address, thread_id, EVENT_EXEC) {
        return;
    }
    submit(Event {
        kind: EVENT_EXEC,
        thread_id,
        address,
        arg0: size as u64,
        ..Event::EMPTY
    });
}

/// Capture the application register context before an instruction executes.
/// Components share arg7 so readers can assemble them into one logical frame.
/// This is deliberately opt-in: a full GP + vector-register snapshot is
/// information rich but much more expensive than instruction/memory alone.
pub unsafe extern "C" fn on_rec_registers(
    context: PbContextHandle,
    _user_data: *mut c_void,
) {
    if context.is_null() {
        return;
    }
    let context = context as PbConstContextHandle;
    let mut tid: PbThreadId = 0;
    if pb_pin_thread_id(&mut tid) != PB_OK {
        return;
    }
    let mut rip = 0u64;
    if pb_pin_get_context_reg(context, crate::arch::instr_ptr_reg(), &mut rip) != PB_OK
        || !armed_in_range(rip, tid as u32, EVENT_REG_SNAPSHOT)
    {
        return;
    }
    let frame = CONTEXT_FRAME.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let mut values = [[0u64; CONTEXT_CHUNKS]; CONTEXT_REG_COUNT];
    let mut available = [false; CONTEXT_REG_COUNT];
    for index in 0..CONTEXT_REG_COUNT {
        let reg = context_reg(index);
        let width = context_width(index);
        if width == 0 {
            // Arch-invalid slot (x86 has only 10 GP registers): skip entirely,
            // so no RAX..R15/RIP read is attempted and no fake component is
            // emitted on the wire.
            continue;
        }
        if width == 8 {
            available[index] = pb_pin_get_context_reg(context, reg, &mut values[index][0]) == PB_OK;
        } else {
            let mut bytes = [0u8; CONTEXT_CHUNKS * 8];
            let mut needed = 0u64;
            if pb_pin_get_context_regval(
                context,
                reg,
                bytes.as_mut_ptr(),
                width as u64,
                &mut needed,
            ) == PB_OK
                && needed >= width as u64
            {
                for chunk in 0..(width / 8) {
                    let start = chunk * 8;
                    values[index][chunk] = u64::from_le_bytes(
                        bytes[start..start + 8].try_into().unwrap(),
                    );
                }
                available[index] = true;
            }
        }
    }

    let slot = context_slot(tid as u32);
    let baseline = slot.map(|state| !state.valid.load(Ordering::Acquire)).unwrap_or(true);
    let mut mask_lo = 0u64;
    let mut mask_hi = 0u64;
    for index in 0..CONTEXT_REG_COUNT {
        if !available[index] {
            continue;
        }
        let changed = baseline
            || slot.is_none()
            || (0..(context_width(index) / 8)).any(|chunk| {
                slot.unwrap().values[index * CONTEXT_CHUNKS + chunk]
                    .load(Ordering::Relaxed)
                    != values[index][chunk]
            });
        if changed {
            if index < 64 {
                mask_lo |= 1u64 << index;
            } else {
                mask_hi |= 1u64 << (index - 64);
            }
        }
    }

    let mut complete = submit(Event {
        kind: EVENT_REG_SNAPSHOT,
        thread_id: tid as u32,
        address: rip,
        arg1: mask_lo,
        arg2: mask_hi,
        arg3: if baseline { 1 } else { 2 },
        arg7: frame,
        ..Event::EMPTY
    });

    for index in 0..CONTEXT_REG_COUNT {
        let changed = if index < 64 {
            mask_lo & (1u64 << index) != 0
        } else {
            mask_hi & (1u64 << (index - 64)) != 0
        };
        if !available[index] || !changed {
            continue;
        }
        let width = context_width(index);
        let parts = if width <= 8 { 1 } else { width / 16 };
        for part in 0..parts {
            let chunk = part * 2;
            if !submit(Event {
                kind: EVENT_REG_SNAPSHOT,
                thread_id: tid as u32,
                address: rip,
                arg0: context_reg(index) as u64,
                arg1: values[index][chunk],
                arg2: values[index][chunk + 1],
                arg3: width as u64,
                arg4: part as u64,
                arg7: frame,
                ..Event::EMPTY
            }) {
                complete = false;
            }
        }
    }

    if complete {
        if let Some(state) = slot {
            for index in 0..CONTEXT_REG_COUNT {
                if !available[index] {
                    continue;
                }
                for chunk in 0..(context_width(index) / 8) {
                    state.values[index * CONTEXT_CHUNKS + chunk]
                        .store(values[index][chunk], Ordering::Relaxed);
                }
            }
            state.valid.store(true, Ordering::Release);
        }
    }
}

pub unsafe extern "C" fn on_rec_memory(
    instruction_address: u64,
    thread_id: u32,
    memory_address: u64,
    size: u32,
    access: u32,
    _user_data: *mut c_void,
) {
    if !armed_in_range(instruction_address, thread_id, EVENT_MEMORY) {
        return;
    }
    submit(Event {
        kind: EVENT_MEMORY,
        thread_id,
        address: instruction_address,
        arg0: memory_address,
        arg1: size as u64,
        arg2: access as u64,
        ..Event::EMPTY
    });
}

pub unsafe extern "C" fn on_rec_branch(
    address: u64,
    thread_id: u32,
    target_address: u64,
    taken: u64,
    _user_data: *mut c_void,
) {
    if !armed_in_range(address, thread_id, EVENT_BRANCH_EDGE) {
        return;
    }
    submit(Event {
        kind: EVENT_BRANCH_EDGE,
        thread_id,
        address,
        arg0: target_address,
        arg1: taken,
        ..Event::EMPTY
    });
}

// ---- instrumentation side (called from engines::on_ins) ----

/// Instrumentation-time gate: insertions land when a session is armed or a
/// past session made them sticky for this range.
#[inline]
pub fn instrumentation_enabled() -> bool {
    ARMED.load(Ordering::Acquire) || STICKY.load(Ordering::Relaxed)
}

#[inline]
pub fn range() -> (u64, u64) {
    (
        RECORD_LO.load(Ordering::Relaxed),
        RECORD_HI.load(Ordering::Relaxed),
    )
}

/// Returns a conservative bounding range for instrumentation. Exact gaps in
/// a multi-range spec are rejected by `in_record_ranges` on the hot path.
pub fn instrumentation_range() -> (u64, u64) {
    let count = RECORD_RANGE_COUNT.load(Ordering::Relaxed);
    if count == 0 {
        return range();
    }
    let mut lo = u64::MAX;
    let mut hi = 0;
    for index in 0..count.min(MAX_RECORD_RANGES) {
        lo = lo.min(RECORD_RANGE_LO[index].load(Ordering::Relaxed));
        hi = hi.max(RECORD_RANGE_HI[index].load(Ordering::Relaxed));
    }
    (lo, hi)
}

/// Inserts the record capture calls for one instruction. Runs at
/// instrumentation time (allocation-friendly); `branchy` mirrors the main
/// engines' branch/call/ret classification for the branch capture.
pub unsafe fn instrument(ins: PbInsHandle, branchy: bool) {
    let mask = RECORD_KINDS.load(Ordering::Relaxed) as u32;
    if mask & (1 << EVENT_REG_SNAPSHOT) != 0 {
        pb_ins_insert_call_before_ctx(ins, Some(on_rec_registers), core::ptr::null_mut());
    }
    if mask & (1 << EVENT_EXEC_BYTES) != 0 {
        pb_ins_insert_capture_exec_bytes(ins, Some(on_rec_exec_bytes), core::ptr::null_mut());
    }
    if mask & (1 << EVENT_MEM_VALUE) != 0 {
        pb_ins_insert_memory_operands_values(ins, Some(on_rec_mem_value), core::ptr::null_mut());
    }
    if mask & (1 << EVENT_EXEC) != 0 {
        pb_ins_insert_exec(ins, Some(on_rec_exec), core::ptr::null_mut());
    }
    if mask & (1 << EVENT_MEMORY) != 0 {
        pb_ins_insert_memory_operands(ins, Some(on_rec_memory), core::ptr::null_mut());
    }
    if mask & (1 << EVENT_BRANCH_EDGE) != 0 && branchy {
        pb_ins_insert_branch_edge(ins, Some(on_rec_branch), core::ptr::null_mut());
    }
}

// ---- session control (query-server thread only) ----

/// (active, recorded, dropped) snapshot for TRACE_STATUS.
pub fn status() -> (bool, u64, u64) {
    (
        ARMED.load(Ordering::Acquire),
        WRITTEN.load(Ordering::Acquire),
        DROPPED.load(Ordering::Relaxed),
    )
}

pub fn session_state() -> u8 {
    if ARMED.load(Ordering::Acquire) {
        SESSION_RECORDING
    } else if !DRAIN_DONE.load(Ordering::Acquire) {
        SESSION_DRAINING
    } else if FAILED.load(Ordering::Acquire) {
        SESSION_FAILED
    } else if SESSION_STARTED.load(Ordering::Acquire) {
        SESSION_COMPLETE
    } else {
        SESSION_IDLE
    }
}

/// Allocates the slab once (first trace_start). try_reserve keeps an OOM
/// from aborting the target process.
fn ensure_slab() -> Result<(), String> {
    if !SLAB.load(Ordering::Acquire).is_null() {
        return Ok(());
    }
    let cap = std::env::var("PINBRIDGE_AGENT_RECORD_CAP")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v >= 1024)
        .unwrap_or(DEFAULT_CAP);
    let mut tags: Vec<u64> = Vec::new();
    tags.try_reserve_exact(cap)
        .map_err(|_| format!("record slab tag alloc failed (cap {cap})"))?;
    tags.resize(cap, 0);
    let mut payloads: Vec<u64> = Vec::new();
    payloads
        .try_reserve_exact(cap * SLOT_WORDS)
        .map_err(|_| format!("record slab payload alloc failed (cap {cap})"))?;
    payloads.resize(cap * SLOT_WORDS, 0);
    let slab = Box::new(Slab {
        tags: tags.into_boxed_slice(),
        payloads: payloads.into_boxed_slice(),
    });
    SLAB_CAP.store(cap, Ordering::Relaxed);
    SLAB.store(Box::into_raw(slab), Ordering::Release);
    crate::log::line(&format!(
        "record slab allocated: cap={cap} ({} MiB)",
        cap * 96 / (1024 * 1024)
    ));
    Ok(())
}

struct DrainArgs {
    path: String,
    writer: Option<std::io::BufWriter<std::fs::File>>,
    kinds_mask: u32,
    lo: u64,
    hi: u64,
    ranges: Vec<(u64, u64)>,
    threads: Vec<u32>,
}

#[derive(Clone)]
struct ModuleScope {
    base: u64,
    end: u64,
    is_main: bool,
    name: String,
}

/// trace_start: arm the record path and flush the window's JIT.
/// Runs on the query-server thread (allocation + spawn happen HERE, never
/// in a callback).
pub fn start(kinds_mask: u32, lo: u64, hi: u64, path: String) -> Result<(), String> {
    start_spec(kinds_mask, vec![(lo, hi)], Vec::new(), path)
}

/// Starts a trace with an allowlist of non-overlapping address ranges and
/// optional application thread ids. Empty `threads` means all threads.
pub fn start_spec(
    kinds_mask: u32,
    ranges: Vec<(u64, u64)>,
    threads: Vec<u32>,
    path: String,
) -> Result<(), String> {
    if ARMED.load(Ordering::Acquire) {
        return Err("already recording".to_string());
    }
    if !DRAIN_DONE.load(Ordering::Acquire) {
        return Err("previous recording is still draining".to_string());
    }
    if kinds_mask == 0 || kinds_mask & !RECORDABLE_MASK != 0 {
        return Err(format!(
            "bad kinds mask 0x{kinds_mask:x} (recordable: 0x{RECORDABLE_MASK:x})"
        ));
    }
    if ranges.is_empty() || ranges.len() > MAX_RECORD_RANGES {
        return Err(format!("bad range count (1..={MAX_RECORD_RANGES})"));
    }
    if ranges.iter().any(|(lo, hi)| lo >= hi) {
        return Err("bad range: lo >= hi".to_string());
    }
    if threads.len() > MAX_RECORD_THREADS {
        return Err(format!("too many thread ids (max {MAX_RECORD_THREADS})"));
    }
    if path.is_empty() {
        return Err("empty path".to_string());
    }
    ensure_slab()?;
    // Open synchronously so trace_start cannot report success for a path the
    // drain thread will never be able to create.
    let file = std::fs::File::create(&path)
        .map_err(|error| format!("trace file create {path} failed: {error}"))?;
    // session reset: counters first, then the tag array (drainer only
    // trusts tags after CLAIM/DRAINED are republished)
    CLAIM.store(0, Ordering::Relaxed);
    DRAINED.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    WRITTEN.store(0, Ordering::Relaxed);
    DRAIN_DONE.store(false, Ordering::Relaxed);
    IN_FLIGHT.store(0, Ordering::Relaxed);
    FAILED.store(false, Ordering::Relaxed);
    SESSION_STARTED.store(true, Ordering::Relaxed);
    reset_context_state();
    unsafe {
        let slab = &*SLAB.load(Ordering::Acquire);
        core::ptr::write_bytes(slab.tags.as_ptr() as *mut u8, 0, SLAB_CAP.load(Ordering::Relaxed) * 8);
    }
    let (lo, hi) = ranges.iter().fold((u64::MAX, 0), |(lo0, hi0), (lo1, hi1)| {
        (lo0.min(*lo1), hi0.max(*hi1))
    });
    RECORD_KINDS.store(kinds_mask as u64, Ordering::Relaxed);
    RECORD_LO.store(lo, Ordering::Relaxed);
    RECORD_HI.store(hi, Ordering::Relaxed);
    for index in 0..MAX_RECORD_RANGES {
        let (range_lo, range_hi) = ranges.get(index).copied().unwrap_or((0, 0));
        RECORD_RANGE_LO[index].store(range_lo, Ordering::Relaxed);
        RECORD_RANGE_HI[index].store(range_hi, Ordering::Relaxed);
    }
    RECORD_RANGE_COUNT.store(ranges.len(), Ordering::Release);
    for index in 0..MAX_RECORD_THREADS {
        RECORD_THREADS[index].store(threads.get(index).copied().unwrap_or(0), Ordering::Relaxed);
    }
    RECORD_THREAD_COUNT.store(threads.len(), Ordering::Release);

    let flush_ranges = ranges.clone();
    let args = Box::new(DrainArgs {
        path,
        writer: Some(std::io::BufWriter::with_capacity(1 << 20, file)),
        kinds_mask,
        lo,
        hi,
        ranges,
        threads,
    });
    let mut thread_id: PbThreadId = 0;
    let mut thread_uid: PbPinThreadUid = 0;
    // Publish the producer gate before spawning: the drainer must never see a
    // false gate and exit before start_spec finishes launching it.
    ARMED.store(true, Ordering::Release);
    let raw_args = Box::into_raw(args);
    let status = unsafe {
        pb_pin_spawn_internal_thread(
            Some(drain_main),
            raw_args as *mut c_void,
            0,
            &mut thread_id,
            &mut thread_uid,
        )
    };
    if status != PB_OK {
        ARMED.store(false, Ordering::Release);
        FAILED.store(true, Ordering::Release);
        // A callback may have passed the producer gate between ARMED being
        // published and the spawn failure. Do not expose the slab for reuse
        // until every such producer has published its reserved slot.
        while IN_FLIGHT.load(Ordering::Acquire) != 0 {
            unsafe {
                pb_pin_sleep(1);
            }
        }
        let claim = CLAIM.load(Ordering::Acquire);
        let drained = DRAINED.load(Ordering::Acquire);
        if claim > drained {
            DROPPED.fetch_add(claim - drained, Ordering::Relaxed);
            DRAINED.store(claim, Ordering::Release);
        }
        unsafe {
            drop(Box::from_raw(raw_args));
        }
        DRAIN_DONE.store(true, Ordering::Release);
        return Err(format!("drain thread spawn failed -> {status}"));
    }
    STICKY.store(true, Ordering::Relaxed);
    unsafe {
        for (range_lo, range_hi) in &flush_ranges {
            let status = pb_pin_remove_instrumentation_in_range(*range_lo, *range_hi);
            if status != PB_OK {
                FAILED.store(true, Ordering::Release);
                stop();
                return Err(format!(
                    "trace instrumentation flush 0x{range_lo:x}-0x{range_hi:x} failed -> {status}"
                ));
            }
        }
    }
    crate::log::line(&format!(
        "trace start kinds=0x{kinds_mask:x} range=0x{lo:x}-0x{hi:x}"
    ));
    Ok(())
}

/// Extends an armed session with temporary ranges. The range slots are
/// populated before the published count, so producer callbacks either see
/// the old complete allowlist or the new complete allowlist. A marker is
/// submitted for each addition so replay can reconstruct the live scope.
pub fn extend_ranges(ranges: Vec<(u64, u64)>) -> Result<(), String> {
    if !ARMED.load(Ordering::Acquire) {
        return Err("not recording".to_string());
    }
    if ranges.is_empty() || ranges.len() > MAX_RECORD_RANGES {
        return Err(format!("bad extension count (1..={MAX_RECORD_RANGES})"));
    }
    if ranges.iter().any(|(lo, hi)| lo >= hi) {
        return Err("bad extension range: lo >= hi".to_string());
    }
    let old_count = RECORD_RANGE_COUNT.load(Ordering::Acquire);
    if old_count + ranges.len() > MAX_RECORD_RANGES {
        return Err(format!(
            "too many live ranges ({} + {} > {MAX_RECORD_RANGES})",
            old_count,
            ranges.len()
        ));
    }
    let new_count = old_count + ranges.len();
    let mut bound_lo = RECORD_LO.load(Ordering::Relaxed);
    let mut bound_hi = RECORD_HI.load(Ordering::Relaxed);
    for (offset, (lo, hi)) in ranges.iter().copied().enumerate() {
        let index = old_count + offset;
        RECORD_RANGE_LO[index].store(lo, Ordering::Relaxed);
        RECORD_RANGE_HI[index].store(hi, Ordering::Relaxed);
        bound_lo = bound_lo.min(lo);
        bound_hi = bound_hi.max(hi);
    }
    RECORD_LO.store(bound_lo, Ordering::Relaxed);
    RECORD_HI.store(bound_hi, Ordering::Relaxed);
    RECORD_RANGE_COUNT.store(new_count, Ordering::Release);
    unsafe {
        for (lo, hi) in &ranges {
            // Re-JIT code already present in the new region. Future code is
            // covered by the sticky instrumentation gate and exact filter.
            let status = pb_pin_remove_instrumentation_in_range(*lo, *hi);
            if status != PB_OK {
                FAILED.store(true, Ordering::Release);
                stop();
                return Err(format!(
                    "trace extension flush 0x{lo:x}-0x{hi:x} failed -> {status}"
                ));
            }
        }
    }
    for (lo, hi) in ranges {
        submit(Event {
            kind: EVENT_MARKER,
            address: 0,
            arg0: MARKER_SCOPE_ADD,
            arg1: lo,
            arg2: hi,
            ..Event::EMPTY
        });
    }
    crate::log::line(&format!(
        "trace extend ranges={} live_ranges={}",
        new_count,
        new_count
    ));
    Ok(())
}

/// trace_stop: disarm and wait (bounded, ~5s) for the drainer to catch up.
/// Idempotent: not recording just reports the last session's counters.
pub fn stop() -> (u64, u64) {
    let was_armed = ARMED.swap(false, Ordering::AcqRel);
    if was_armed || !DRAIN_DONE.load(Ordering::Acquire) {
        for _ in 0..1000 {
            if DRAIN_DONE.load(Ordering::Acquire) {
                break;
            }
            unsafe {
                pb_pin_sleep(5);
            }
        }
        if !DRAIN_DONE.load(Ordering::Acquire) {
            crate::log::line(
                "trace stop timed out: drainer still owns the session; restart is blocked",
            );
        }
        crate::log::line(&format!(
            "trace stop recorded={} dropped={}",
            WRITTEN.load(Ordering::Acquire),
            DROPPED.load(Ordering::Relaxed)
        ));
    }
    (
        WRITTEN.load(Ordering::Acquire),
        DROPPED.load(Ordering::Relaxed),
    )
}

// ---- drain thread (Pin internal; std io + allocation are fine here) ----

fn main_module_name() -> String {
    unsafe {
        let mut img = PbImgHandle { opaque: 0 };
        if pb_app_img_head(&mut img) != PB_OK {
            return String::new();
        }
        let mut valid: u8 = 0;
        pb_img_valid(img, &mut valid);
        while valid != 0 {
            let mut is_main: u8 = 0;
            pb_img_is_main_executable(img, &mut is_main);
            if is_main != 0 {
                let mut buf = [0 as std::os::raw::c_char; 512];
                let mut needed: u64 = 0;
                if pb_img_name(img, buf.as_mut_ptr(), 512, &mut needed) == PB_OK {
                    let full = std::ffi::CStr::from_ptr(buf.as_ptr())
                        .to_string_lossy()
                        .into_owned();
                    return full
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or(&full)
                        .to_string();
                }
                return String::new();
            }
            let mut next = PbImgHandle { opaque: 0 };
            if pb_img_next(img, &mut next) != PB_OK {
                break;
            }
            img = next;
            valid = 0;
            pb_img_valid(img, &mut valid);
        }
    }
    String::new()
}

/// Snapshot modules intersecting the requested address ranges. The recorder
/// accepts absolute ranges, so this metadata makes the module selection
/// auditable even when the target has several loaded images.
fn scoped_modules(ranges: &[(u64, u64)]) -> Vec<ModuleScope> {
    let mut modules = Vec::new();
    unsafe {
        let mut img = PbImgHandle { opaque: 0 };
        if pb_app_img_head(&mut img) != PB_OK {
            return modules;
        }
        let mut valid: u8 = 0;
        pb_img_valid(img, &mut valid);
        while valid != 0 && modules.len() < 512 {
            let mut base = 0u64;
            let mut end = 0u64;
            pb_img_low_address(img, &mut base);
            pb_img_high_address(img, &mut end);
            if ranges.iter().any(|(lo, hi)| *lo < end && *hi > base) {
                let mut is_main = 0u8;
                pb_img_is_main_executable(img, &mut is_main);
                let mut buf = [0 as std::os::raw::c_char; 512];
                let mut needed = 0u64;
                let name = if pb_img_name(img, buf.as_mut_ptr(), 512, &mut needed) == PB_OK {
                    std::ffi::CStr::from_ptr(buf.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                };
                modules.push(ModuleScope {
                    base,
                    end,
                    is_main: is_main != 0,
                    name,
                });
            }
            let mut next = PbImgHandle { opaque: 0 };
            if pb_img_next(img, &mut next) != PB_OK {
                break;
            }
            img = next;
            valid = 0;
            pb_img_valid(img, &mut valid);
        }
    }
    modules
}

/// Seconds-since-epoch -> "YYYY-MM-DDTHH:MM:SSZ" (civil-from-days, no deps).
fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86400) as i64;
    let day_secs = secs % 86400;
    // Howard Hinnant's civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        day_secs / 3600,
        (day_secs / 60) % 60,
        day_secs % 60
    )
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn to_record(event: &Event) -> pinbridge_proto::EventRecord {
    pinbridge_proto::EventRecord {
        sequence: event.sequence,
        kind: event.kind,
        thread_id: event.thread_id,
        address: event.address,
        arg0: event.arg0,
        arg1: event.arg1,
        arg2: event.arg2,
        arg3: event.arg3,
        arg4: event.arg4,
        arg5: event.arg5,
        arg6: event.arg6,
        arg7: event.arg7,
    }
}

fn marker(sequence: u64, tag: u64, value: u64) -> Event {
    Event {
        sequence,
        kind: EVENT_MARKER,
        address: 0,
        arg0: tag,
        arg1: value,
        ..Event::EMPTY
    }
}

fn write_record(out: &mut Vec<u8>, event: &Event) {
    out.clear();
    to_record(event).encode(out);
}

fn same_payload(left: &Event, right: &Event) -> bool {
    left.kind == right.kind
        && left.thread_id == right.thread_id
        && left.address == right.address
        && left.arg0 == right.arg0
        && left.arg1 == right.arg1
        && left.arg2 == right.arg2
        && left.arg3 == right.arg3
        && left.arg4 == right.arg4
        && left.arg5 == right.arg5
        && left.arg6 == right.arg6
        && left.arg7 == right.arg7
}

/// Flush one run. The first event is emitted verbatim; a repeat marker then
/// says how many additional logical events have the same payload. Its
/// sequence is the final logical sequence number in the run, so a reader can
/// reconstruct exact sequence positions without storing every copy.
fn flush_run<W: Write>(
    writer: &mut W,
    scratch: &mut Vec<u8>,
    event: &Event,
    repeat_count: u64,
) -> std::io::Result<()> {
    write_record(scratch, event);
    writer.write_all(scratch)?;
    if repeat_count > 0 {
        let repeat = Event {
            sequence: event.sequence.saturating_add(repeat_count),
            kind: EVENT_REPEAT,
            thread_id: event.thread_id,
            address: event.address,
            arg0: repeat_count,
            arg1: event.kind as u64,
            ..Event::EMPTY
        };
        write_record(scratch, &repeat);
        writer.write_all(scratch)?;
    }
    Ok(())
}

struct DrainCompletion;

impl Drop for DrainCompletion {
    fn drop(&mut self) {
        // Published only after writer/args locals have been dropped and the
        // output handle is closed (locals declared after this guard drop first).
        DRAIN_DONE.store(true, Ordering::Release);
    }
}

fn fail_drain(reason: &str) {
    FAILED.store(true, Ordering::Release);
    ARMED.store(false, Ordering::Release);
    while IN_FLIGHT.load(Ordering::Acquire) != 0 {
        unsafe {
            pb_pin_sleep(1);
        }
    }
    let claim = CLAIM.load(Ordering::Acquire);
    let drained = DRAINED.load(Ordering::Acquire);
    if claim > drained {
        DROPPED.fetch_add(claim - drained, Ordering::Relaxed);
        DRAINED.store(claim, Ordering::Release);
    }
    crate::log::line(reason);
}

unsafe extern "C" fn drain_main(argument: *mut c_void) {
    let _completion = DrainCompletion;
    let mut args = Box::from_raw(argument as *mut DrainArgs);
    let slab = &*SLAB.load(Ordering::Acquire);
    let cap = SLAB_CAP.load(Ordering::Relaxed);
    let Some(mut writer) = args.writer.take() else {
        fail_drain("trace record: drain thread started without a writer");
        return;
    };

    let kinds: Vec<u32> = (0..32)
        .filter(|k| args.kinds_mask & (1 << k) != 0)
        .collect();
    let kinds_json = kinds
        .iter()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let modules_json = scoped_modules(&args.ranges)
        .iter()
        .map(|module| {
            format!(
                "{{\"base\":{},\"end\":{},\"is_main\":{},\"name\":\"{}\"}}",
                module.base,
                module.end,
                if module.is_main { "true" } else { "false" },
                json_escape(&module.name)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let meta = format!(
        "{{\"target\":\"{}\",\"created\":\"{}\",\"kinds\":[{}],\"agent\":\"pinbridge-agent\",\"arch\":\"{}\",\"pointer_width\":{},\"format\":{{\"version\":1,\"repeat_kind\":12,\"repeat_encoding\":\"rle\",\"reg_snapshot_kind\":13}},\"modules\":[{}],\"ranges\":[{}],\"threads\":[{}],\"note\":\"window [0x{:x},0x{:x}); exact multi-range/thread filtering is applied before ring claim; skip unknown kinds, tolerate truncated tail\"}}",
        json_escape(&main_module_name()),
        iso8601_now(),
        kinds_json,
        crate::arch::name(),
        crate::arch::pointer_width(),
        modules_json,
        args.ranges
            .iter()
            .map(|(lo, hi)| format!("[{lo},{hi}]"))
            .collect::<Vec<_>>()
            .join(","),
        args.threads.iter().map(|tid| tid.to_string()).collect::<Vec<_>>().join(","),
        args.lo,
        args.hi
    );
    let mut scratch: Vec<u8> = Vec::with_capacity(4096);
    scratch.extend_from_slice(b"PBTR");
    scratch.extend_from_slice(&1u32.to_le_bytes()); // version
    scratch.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    scratch.extend_from_slice(&0u32.to_le_bytes()); // reserved
    scratch.extend_from_slice(meta.as_bytes());
    if writer.write_all(&scratch).is_err() {
        fail_drain("trace record: header write failed");
        return;
    }
    scratch.clear();
    scratch.reserve(pinbridge_proto::EVENT_WIRE_LEN);

    write_record(&mut scratch, &marker(0, MARKER_START, args.kinds_mask as u64));
    if writer.write_all(&scratch).is_err() {
        fail_drain("trace record: start marker write failed");
        return;
    }

    let mut next: u64 = 0;
    let mut pending: Option<Event> = None;
    let mut pending_repeats: u64 = 0;
    let mut io_failed = false;
    loop {
        let claim = CLAIM.load(Ordering::Acquire);
        if next < claim {
            let slot = (next % cap as u64) as usize;
            let tag = &*(slab.tags.as_ptr().add(slot) as *const AtomicU64);
            if tag.load(Ordering::Acquire) == next + 1 {
                let event = core::ptr::read(
                    slab.payloads.as_ptr().add(slot * SLOT_WORDS) as *const Event
                );
                match pending.take() {
                    Some(previous) => {
                        let expected = previous
                            .sequence
                            .saturating_add(pending_repeats)
                            .saturating_add(1);
                        if expected == event.sequence && same_payload(&previous, &event) {
                            pending = Some(previous);
                            pending_repeats = pending_repeats.saturating_add(1);
                        } else {
                            if flush_run(&mut writer, &mut scratch, &previous, pending_repeats).is_err() {
                                crate::log::line("trace record: file write failed, stopping drain");
                                io_failed = true;
                                ARMED.store(false, Ordering::Release);
                                break;
                            }
                            pending = Some(event);
                            pending_repeats = 0;
                        }
                    }
                    None => {
                        pending = Some(event);
                    }
                }
                // WRITTEN is the number of logical capture events, not the
                // number of physical records after run-length encoding.
                WRITTEN.fetch_add(1, Ordering::Relaxed);
                next += 1;
                DRAINED.store(next, Ordering::Release);
            } else {
                // Every reserved claim has a live producer. Once all producers
                // have left, a missing tag is an invariant failure rather than
                // a normal overflow condition.
                if !ARMED.load(Ordering::Acquire)
                    && IN_FLIGHT.load(Ordering::Acquire) == 0
                {
                    crate::log::line("trace record: unpublished reserved slot; marking incomplete");
                    FAILED.store(true, Ordering::Release);
                    DROPPED.fetch_add(1, Ordering::Relaxed);
                    next += 1;
                    DRAINED.store(next, Ordering::Release);
                } else {
                    pb_pin_sleep(1);
                }
            }
        } else if !ARMED.load(Ordering::Acquire)
            && IN_FLIGHT.load(Ordering::Acquire) == 0
        {
            break; // stopped and caught up
        } else {
            pb_pin_sleep(1);
        }
    }

    if !io_failed {
        if let Some(previous) = pending.as_ref() {
            if flush_run(&mut writer, &mut scratch, previous, pending_repeats).is_err() {
                crate::log::line("trace record: final run write failed");
                io_failed = true;
            }
        }
    }
    if io_failed {
        FAILED.store(true, Ordering::Release);
        while IN_FLIGHT.load(Ordering::Acquire) != 0 {
            pb_pin_sleep(1);
        }
        let claim = CLAIM.load(Ordering::Acquire);
        if claim > next {
            DROPPED.fetch_add(claim - next, Ordering::Relaxed);
            DRAINED.store(claim, Ordering::Release);
        }
    }
    let recorded = WRITTEN.load(Ordering::Relaxed);
    write_record(&mut scratch, &marker(next + 1, MARKER_STOP, recorded));
    if writer.write_all(&scratch).is_err() || writer.flush().is_err() {
        FAILED.store(true, Ordering::Release);
        crate::log::line("trace record: stop marker/flush failed");
    }
    crate::log::line(&format!(
        "trace record: {} closed (recorded={recorded} dropped={})",
        args.path,
        DROPPED.load(Ordering::Relaxed)
    ));
}

#[cfg(test)]
mod tests {
    use super::try_reserve;
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn full_reservation_does_not_create_a_sequence_hole() {
        let claim = AtomicU64::new(0);
        let drained = AtomicU64::new(0);

        assert_eq!(try_reserve(&claim, &drained, 2), Some(0));
        assert_eq!(try_reserve(&claim, &drained, 2), Some(1));
        assert_eq!(try_reserve(&claim, &drained, 2), None);
        assert_eq!(claim.load(Ordering::Relaxed), 2);

        drained.store(1, Ordering::Release);
        assert_eq!(try_reserve(&claim, &drained, 2), Some(2));
        assert_eq!(claim.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn concurrent_reservations_are_unique_and_contiguous() {
        const WORKERS: usize = 8;
        const PER_WORKER: usize = 256;
        let claim = Arc::new(AtomicU64::new(0));
        let drained = Arc::new(AtomicU64::new(0));
        let mut workers = Vec::new();

        for _ in 0..WORKERS {
            let claim = Arc::clone(&claim);
            let drained = Arc::clone(&drained);
            workers.push(std::thread::spawn(move || {
                let mut reservations = Vec::with_capacity(PER_WORKER);
                for _ in 0..PER_WORKER {
                    reservations.push(
                        try_reserve(&claim, &drained, (WORKERS * PER_WORKER) as u64)
                            .expect("capacity covers every reservation"),
                    );
                }
                reservations
            }));
        }

        let mut reservations = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker panicked"))
            .collect::<Vec<_>>();
        reservations.sort_unstable();
        assert_eq!(
            reservations,
            (0..(WORKERS * PER_WORKER) as u64).collect::<Vec<_>>()
        );
    }
}
