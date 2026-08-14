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
//! locks, no I/O): claim = CLAIM.fetch_add; drop (counted) when the claim
//! lapped the drainer by a full buffer; re-check ARMED post-claim and
//! abandon the slot when stop raced us (the drainer skips the hole after a
//! bounded wait); write the 88-byte payload, then the tag (claim + 1) with
//! Release ordering.
//!
//! Drainer (Pin internal thread, std io allowed there): polls tags in
//! order, appends 88-byte wire-layout records (pinbridge-proto EventRecord)
//! to the file, finishes after trace_stop once caught up. Kind-11 marker
//! records bracket the tape (start/end annotations).
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
//!   then 88-byte EventRecord wire images. entry_context is omitted in v1:
//!   snapshotting RUNNING threads is unsafe, so the replay reader must
//!   tolerate its absence (see taint-roadmap).

use crate::event::{
    Event, EVENT_BRANCH_EDGE, EVENT_EXEC, EVENT_EXEC_BYTES, EVENT_MARKER, EVENT_MEMORY,
    EVENT_MEM_VALUE,
};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicPtr, AtomicUsize, Ordering};
use pinbridge_sys::*;
use std::io::Write;

/// Default slot capacity of the record slab (one session's tape).
const DEFAULT_CAP: usize = 1_000_000;
/// u64 words per slot payload (88-byte Event).
const SLOT_WORDS: usize = 11;
/// Marker tags (kind 11, arg0).
const MARKER_START: u64 = 1;
const MARKER_STOP: u64 = 2;
/// Kinds the record channel can capture (v1): memory, exec, branch,
/// exec_bytes, mem_value. syscall/hook/module stay main-ring only.
pub const RECORDABLE_MASK: u32 = (1 << EVENT_MEMORY)
    | (1 << EVENT_EXEC)
    | (1 << EVENT_BRANCH_EDGE)
    | (1 << EVENT_EXEC_BYTES)
    | (1 << EVENT_MEM_VALUE);

struct Slab {
    /// Per-slot sequence tag (plain u64 storage, accessed as AtomicU64 —
    /// same size/alignment/validity, so the cast is sound).
    tags: Box<[u64]>,
    /// cap * SLOT_WORDS words of payload storage (one Event per slot).
    payloads: Box<[u64]>,
}

static SLAB: AtomicPtr<Slab> = AtomicPtr::new(core::ptr::null_mut());
static SLAB_CAP: AtomicUsize = AtomicUsize::new(0);

/// Claims issued this session (producers).
static CLAIM: AtomicU64 = AtomicU64::new(0);
/// Drainer cursor: slots consumed (or hole-skipped) this session.
static DRAINED: AtomicU64 = AtomicU64::new(0);
/// Overflow drops + abandoned/hole-skipped slots this session.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// Records actually written to the file this session.
static WRITTEN: AtomicU64 = AtomicU64::new(0);
/// Recording live: producers may claim. Cleared by trace_stop.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Drainer finished (file closed, caught up). Set once per session.
static DRAIN_DONE: AtomicBool = AtomicBool::new(false);
/// Sticky instrumentation flag: once a session armed, record insertions
/// keep landing in newly JITted code for the armed range (inert while
/// ARMED is down — the analysis-time re-check gates them).
static STICKY: AtomicBool = AtomicBool::new(false);
/// Runtime record window + kind mask (instrumentation- and analysis-time).
static RECORD_LO: AtomicU64 = AtomicU64::new(0);
static RECORD_HI: AtomicU64 = AtomicU64::new(0);
static RECORD_KINDS: AtomicU64 = AtomicU64::new(0);

// ---- producer side (analysis callbacks; hot-path discipline) ----

/// Analysis-time gate: recording live + address inside the window + this
/// kind requested. Already-JITted code with stale insertions dies here.
#[inline]
fn armed_in_range(address: u64, kind: u32) -> bool {
    ARMED.load(Ordering::Acquire)
        && (RECORD_KINDS.load(Ordering::Relaxed) & (1 << kind) as u64) != 0
        && address >= RECORD_LO.load(Ordering::Relaxed)
        && address < RECORD_HI.load(Ordering::Relaxed)
}

/// Hot-path entry point: record one event into the slab. Never blocks;
/// overflow and post-stop claims drop (counted).
#[inline]
pub fn submit(mut event: Event) {
    let slab = SLAB.load(Ordering::Acquire);
    if slab.is_null() {
        return;
    }
    let cap = SLAB_CAP.load(Ordering::Relaxed);
    let claim = CLAIM.fetch_add(1, Ordering::Relaxed);
    if claim.wrapping_sub(DRAINED.load(Ordering::Acquire)) >= cap as u64 {
        // buffer lapped: the drainer is a full slab behind — drop.
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    if !ARMED.load(Ordering::Acquire) {
        // stop raced this claim: abandon the slot (hole-skipped by drainer).
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
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
}

pub unsafe extern "C" fn on_rec_exec_bytes(
    address: u64,
    thread_id: u32,
    size: u32,
    bytes_lo: u64,
    bytes_hi: u64,
    _user_data: *mut c_void,
) {
    if !armed_in_range(address, EVENT_EXEC_BYTES) {
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
    if !armed_in_range(instruction_address, EVENT_MEM_VALUE) {
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
    if !armed_in_range(address, EVENT_EXEC) {
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

pub unsafe extern "C" fn on_rec_memory(
    instruction_address: u64,
    thread_id: u32,
    memory_address: u64,
    size: u32,
    access: u32,
    _user_data: *mut c_void,
) {
    if !armed_in_range(instruction_address, EVENT_MEMORY) {
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
    if !armed_in_range(address, EVENT_BRANCH_EDGE) {
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

/// Inserts the record capture calls for one instruction. Runs at
/// instrumentation time (allocation-friendly); `branchy` mirrors the main
/// engines' branch/call/ret classification for the branch capture.
pub unsafe fn instrument(ins: PbInsHandle, branchy: bool) {
    let mask = RECORD_KINDS.load(Ordering::Relaxed) as u32;
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
    kinds_mask: u32,
    lo: u64,
    hi: u64,
}

/// trace_start: arm the record path and flush the window's JIT.
/// Runs on the query-server thread (allocation + spawn happen HERE, never
/// in a callback).
pub fn start(kinds_mask: u32, lo: u64, hi: u64, path: String) -> Result<(), String> {
    if ARMED.load(Ordering::Acquire) {
        return Err("already recording".to_string());
    }
    if kinds_mask == 0 || kinds_mask & !RECORDABLE_MASK != 0 {
        return Err(format!(
            "bad kinds mask 0x{kinds_mask:x} (recordable: 0x{RECORDABLE_MASK:x})"
        ));
    }
    if lo >= hi {
        return Err("bad range: lo >= hi".to_string());
    }
    if path.is_empty() {
        return Err("empty path".to_string());
    }
    ensure_slab()?;
    // session reset: counters first, then the tag array (drainer only
    // trusts tags after CLAIM/DRAINED are republished)
    CLAIM.store(0, Ordering::Relaxed);
    DRAINED.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    WRITTEN.store(0, Ordering::Relaxed);
    DRAIN_DONE.store(false, Ordering::Relaxed);
    unsafe {
        let slab = &*SLAB.load(Ordering::Acquire);
        core::ptr::write_bytes(slab.tags.as_ptr() as *mut u8, 0, SLAB_CAP.load(Ordering::Relaxed) * 8);
    }
    RECORD_KINDS.store(kinds_mask as u64, Ordering::Relaxed);
    RECORD_LO.store(lo, Ordering::Relaxed);
    RECORD_HI.store(hi, Ordering::Relaxed);

    let args = Box::new(DrainArgs {
        path,
        kinds_mask,
        lo,
        hi,
    });
    let mut thread_id: PbThreadId = 0;
    let mut thread_uid: PbPinThreadUid = 0;
    let status = unsafe {
        pb_pin_spawn_internal_thread(
            Some(drain_main),
            Box::into_raw(args) as *mut c_void,
            0,
            &mut thread_id,
            &mut thread_uid,
        )
    };
    if status != PB_OK {
        return Err(format!("drain thread spawn failed -> {status}"));
    }
    // Arm BEFORE the JIT flush: re-instrumented code must see the open gate
    // (instrumentation-time check reads ARMED || STICKY).
    ARMED.store(true, Ordering::Release);
    STICKY.store(true, Ordering::Relaxed);
    unsafe {
        pb_pin_remove_instrumentation_in_range(lo, hi);
    }
    crate::log::line(&format!(
        "trace start kinds=0x{kinds_mask:x} range=0x{lo:x}-0x{hi:x}"
    ));
    Ok(())
}

/// trace_stop: disarm and wait (bounded, ~5s) for the drainer to catch up.
/// Idempotent: not recording just reports the last session's counters.
pub fn stop() -> (u64, u64) {
    if ARMED.swap(false, Ordering::AcqRel) {
        for _ in 0..1000 {
            if DRAIN_DONE.load(Ordering::Acquire) {
                break;
            }
            unsafe {
                pb_pin_sleep(5);
            }
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

/// Hole patience before skipping an abandoned slot: ~64ms of 1ms naps.
/// A preempted producer resolves in microseconds; only the post-stop
/// abandon race produces a real hole.
const HOLE_NAPS: u32 = 64;

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

unsafe extern "C" fn drain_main(argument: *mut c_void) {
    let args = Box::from_raw(argument as *mut DrainArgs);
    let slab = &*SLAB.load(Ordering::Acquire);
    let cap = SLAB_CAP.load(Ordering::Relaxed);

    let file = match std::fs::File::create(&args.path) {
        Ok(file) => file,
        Err(error) => {
            crate::log::line(&format!("trace record: create {} failed: {error}", args.path));
            // Leave nothing claimable: the session is dead on arrival.
            ARMED.store(false, Ordering::Release);
            DRAIN_DONE.store(true, Ordering::Release);
            return;
        }
    };
    let mut writer = std::io::BufWriter::with_capacity(1 << 20, file);

    let kinds: Vec<u32> = (0..32)
        .filter(|k| args.kinds_mask & (1 << k) != 0)
        .collect();
    let kinds_json = kinds
        .iter()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let meta = format!(
        "{{\"target\":\"{}\",\"created\":\"{}\",\"kinds\":[{}],\"agent\":\"pinbridge-agent\",\"note\":\"window [0x{:x},0x{:x}); entry_context omitted (unsafe to snapshot running threads); skip unknown kinds, tolerate truncated tail\"}}",
        json_escape(&main_module_name()),
        iso8601_now(),
        kinds_json,
        args.lo,
        args.hi
    );
    let mut scratch: Vec<u8> = Vec::with_capacity(4096);
    scratch.extend_from_slice(b"PBTR");
    scratch.extend_from_slice(&1u32.to_le_bytes()); // version
    scratch.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    scratch.extend_from_slice(&0u32.to_le_bytes()); // reserved
    scratch.extend_from_slice(meta.as_bytes());
    let _ = writer.write_all(&scratch);
    scratch.clear();
    scratch.reserve(pinbridge_proto::EVENT_WIRE_LEN);

    write_record(&mut scratch, &marker(0, MARKER_START, args.kinds_mask as u64));
    let _ = writer.write_all(&scratch);

    let mut next: u64 = 0;
    let mut hole_naps: u32 = 0;
    loop {
        let claim = CLAIM.load(Ordering::Acquire);
        if next < claim {
            let slot = (next % cap as u64) as usize;
            let tag = &*(slab.tags.as_ptr().add(slot) as *const AtomicU64);
            if tag.load(Ordering::Acquire) == next + 1 {
                let event = core::ptr::read(
                    slab.payloads.as_ptr().add(slot * SLOT_WORDS) as *const Event
                );
                write_record(&mut scratch, &event);
                if writer.write_all(&scratch).is_err() {
                    crate::log::line("trace record: file write failed, stopping drain");
                    break;
                }
                WRITTEN.fetch_add(1, Ordering::Relaxed);
                next += 1;
                DRAINED.store(next, Ordering::Release);
                hole_naps = 0;
            } else if hole_naps >= HOLE_NAPS {
                // abandoned claim (stop race): count and skip the hole
                DROPPED.fetch_add(1, Ordering::Relaxed);
                next += 1;
                DRAINED.store(next, Ordering::Release);
                hole_naps = 0;
            } else {
                hole_naps += 1;
                pb_pin_sleep(1);
            }
        } else if !ARMED.load(Ordering::Acquire) {
            break; // stopped and caught up
        } else {
            pb_pin_sleep(1);
        }
    }

    let recorded = WRITTEN.load(Ordering::Relaxed);
    write_record(&mut scratch, &marker(next + 1, MARKER_STOP, recorded));
    let _ = writer.write_all(&scratch);
    let _ = writer.flush();
    DRAIN_DONE.store(true, Ordering::Release);
    crate::log::line(&format!(
        "trace record: {} closed (recorded={recorded} dropped={})",
        args.path,
        DROPPED.load(Ordering::Relaxed)
    ));
}
