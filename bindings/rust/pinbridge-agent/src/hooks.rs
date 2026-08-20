//! Runtime-managed hook point set (HOOK_* ops): addresses that get a
//! register-capture call inserted at instrumentation time. This powers
//! "hook all exports of a module" without spending breakpoint slots.
//! Hits surface as the existing kind-1 (hook_regs) events — this module
//! only decides *where* capture calls get inserted; the analysis callback
//! itself lives in engines.rs (on_hook_regs, fixed four-slot register layout
//! selected by the target architecture).
//!
//! Two copies of the set:
//!   - master: sorted Vec behind a std mutex, touched only by the
//!     query-server/init threads;
//!   - snapshot: immutable boxed sorted Vec behind an atomic pointer, read
//!     lock-free by the instrumentation callback (hot: runs on application
//!     threads during JIT — no locks, no allocation, no I/O there).
//! Writers build a fresh snapshot and swap the pointer; retired snapshots
//! are reclaimed after a read-side quiescent point.
//! Set changes flush the JIT range of the affected address (same re-JIT
//! technique as bp.rs) so instrumentation re-evaluates it.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

/// A full process can easily expose more than 4096 named DLL entries. Keep
/// enough headroom for a 20k workload while retaining a simple fixed policy.
pub const MAX_HOOK_POINTS: usize = 32768;
pub const MAX_HOOK_RULES: usize = 65536;
/// Virtual PbRegId values used only by HookRule for ABI-aware stack args.
pub const HOOK_STACK_ARG_BASE: PbRegId = 0x8000_0000;

#[inline]
fn stack_arg_index(reg: PbRegId) -> Option<u32> {
    reg.checked_sub(HOOK_STACK_ARG_BASE)
        .filter(|index| *index < 1024)
}

/// A synchronous action evaluated on the application thread at a runtime
/// hook.  `match_reg == PB_REG_INVALID_` means unconditional; otherwise the
/// rule fires when `(register & match_mask) == (match_value & match_mask)`.
/// Virtual `stackN` registers address ABI-aware integer stack arguments.
/// `thread_id == PB_INVALID_THREAD_ID` means all threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HookRule {
    pub address: u64,
    pub thread_id: PbThreadId,
    pub match_reg: PbRegId,
    pub match_mask: u64,
    pub match_value: u64,
    pub set_reg: PbRegId,
    pub set_value: u64,
}

static MASTER: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
static SNAPSHOT: AtomicPtr<Vec<u64>> = AtomicPtr::new(core::ptr::null_mut());
/// Lock-free mirror of the master length for the on_ins pre-check.
static COUNT: AtomicUsize = AtomicUsize::new(0);
static SNAPSHOT_READERS: AtomicUsize = AtomicUsize::new(0);
/// Snapshots retired by earlier swaps (as raw addresses; *mut T is not
/// Send), freed on the next update. Touched by the query-server thread only.
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

// Monotonic two-hash Bloom prefilters keep the common non-Hook instruction
// path out of the snapshot read-side critical section. Bits are deliberately
// never cleared: a stale bit only causes an ordinary binary-search fallback,
// while clearing during concurrent Pin JIT callbacks could create a false
// negative. At the 32k Hook capacity the table remains sparsely populated.
const HOOK_BLOOM_WORDS: usize = 16_384;
const HOOK_BLOOM_BITS: usize = HOOK_BLOOM_WORDS * 64;
static HOOK_BLOOM: [AtomicU64; HOOK_BLOOM_WORDS] = [const { AtomicU64::new(0) }; HOOK_BLOOM_WORDS];
static FUNCTION_BLOOM: [AtomicU64; HOOK_BLOOM_WORDS] =
    [const { AtomicU64::new(0) }; HOOK_BLOOM_WORDS];

/// Function-call logging is deliberately separate from ordinary instruction
/// Hooks. Every function entry is also in MASTER, while only addresses armed
/// by HOOK_SET_BATCH are in this subset and receive function-owned return
/// instruction capture.
static FUNCTIONS_MASTER: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
static FUNCTIONS_SNAPSHOT: AtomicPtr<Vec<u64>> = AtomicPtr::new(core::ptr::null_mut());
static FUNCTION_COUNT: AtomicUsize = AtomicUsize::new(0);
static FUNCTIONS_READERS: AtomicUsize = AtomicUsize::new(0);
static FUNCTIONS_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// Compact signature facts needed on the Pin hot path. Names and full C type
/// spellings remain in Hub; Agent only selects integer registers, XMM
/// registers, or stack slots without parsing text in an analysis callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FunctionSignatureLayout {
    pub address: u64,
    pub calling_convention: u32,
    pub return_kind: u32,
    pub parameter_count: u32,
    pub float_parameter_mask: u32,
}

pub const CALL_CONV_DEFAULT: u32 = 0;
pub const CALL_CONV_X86_FASTCALL: u32 = 1;
pub const VALUE_INTEGER: u32 = 0;
pub const VALUE_FLOAT32: u32 = 1;
pub const VALUE_FLOAT64: u32 = 2;

static SIGNATURES_MASTER: std::sync::Mutex<Vec<FunctionSignatureLayout>> =
    std::sync::Mutex::new(Vec::new());
static SIGNATURES_SNAPSHOT: AtomicPtr<Vec<FunctionSignatureLayout>> =
    AtomicPtr::new(core::ptr::null_mut());
static SIGNATURES_READERS: AtomicUsize = AtomicUsize::new(0);
static SIGNATURES_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
static SIGNATURE_COUNT: AtomicUsize = AtomicUsize::new(0);

static RULES_MASTER: std::sync::Mutex<Vec<HookRule>> = std::sync::Mutex::new(Vec::new());
static RULES_SNAPSHOT: AtomicPtr<Vec<HookRule>> = AtomicPtr::new(core::ptr::null_mut());
static RULES_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
static RULES_READERS: AtomicUsize = AtomicUsize::new(0);
static RULE_COUNT: AtomicUsize = AtomicUsize::new(0);
static ACTION_TLS_KEY: AtomicI32 = AtomicI32::new(PB_INVALID_TLS_KEY);
/// Normally zero. It keeps passive monitor callbacks away from Pin TLS while
/// still suppressing the single replay created when a callback changes the
/// context and is downgraded to monitoring before `ExecuteAt` re-enters.
static REPLAY_GUARD_COUNT: AtomicUsize = AtomicUsize::new(0);
const MAX_FUNCTION_CALL_DEPTH: usize = 64;
// Pin documents PIN_MAX_THREADS=8192 for 64-bit Windows/Linux (2048 on the
// remaining targets). Keep function-call pairing in zero-initialized tool
// storage indexed by Pin THREADID. Allocating a Box from the thread-start
// callback can wait on the process heap while Pin holds its client lock;
// protected Windows loaders can then form a startup lock cycle before any
// internal control thread is permitted to run.
const MAX_PIN_THREADS: usize = 8192;

#[derive(Clone, Copy)]
struct FunctionCallFrame {
    function_address: u64,
    entry_stack_pointer: u64,
}

const EMPTY_FUNCTION_CALL_FRAME: FunctionCallFrame = FunctionCallFrame {
    function_address: 0,
    entry_stack_pointer: 0,
};

#[derive(Clone, Copy)]
struct FunctionCallState {
    frames: [FunctionCallFrame; MAX_FUNCTION_CALL_DEPTH],
    depth: usize,
}

impl FunctionCallState {
    const fn new() -> Self {
        Self {
            frames: [EMPTY_FUNCTION_CALL_FRAME; MAX_FUNCTION_CALL_DEPTH],
            depth: 0,
        }
    }
}
static mut FUNCTION_CALL_STATES: [FunctionCallState; MAX_PIN_THREADS] =
    [FunctionCallState::new(); MAX_PIN_THREADS];
/// Whether named Python Hook observers need the native-filtered observation
/// copy. The compatibility telemetry record is always retained for CLI/UI
/// consumers, while Python callbacks consume only this dedicated copy.
static OBSERVATION_ENABLED: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct ObservationInterest {
    address: u64,
    is_return: bool,
}
static OBSERVATION_ALL_MASK: AtomicUsize = AtomicUsize::new(0);
static OBSERVATION_INTERESTS: AtomicPtr<Vec<ObservationInterest>> =
    AtomicPtr::new(core::ptr::null_mut());
static OBSERVATION_READERS: AtomicUsize = AtomicUsize::new(0);
static OBSERVATION_RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

pub fn set_observation_enabled(enabled: bool) {
    OBSERVATION_ENABLED.store(enabled, Ordering::Release);
    // Compatibility/race-safe path used immediately before a new pb.on Hook
    // subscription is committed. The next registry recomputation publishes
    // the exact per-address table and narrows this temporary all-events mask.
    OBSERVATION_ALL_MASK.store(if enabled { 0b11 } else { 0 }, Ordering::Release);
}

#[inline]
pub fn observation_enabled() -> bool {
    OBSERVATION_ENABLED.load(Ordering::Acquire)
}

/// Publishes the exact native filter for asynchronous Hook observation.
/// Raw Hook telemetry still reaches the compatibility ring; only addresses
/// selected by pb.on(...) enter the Python observation lane.
pub fn publish_observation_interests(entry_all: bool, return_all: bool, interests: &[(u64, bool)]) {
    let mut snapshot = interests
        .iter()
        .filter(|(address, _)| *address != 0)
        .map(|(address, is_return)| ObservationInterest {
            address: *address,
            is_return: *is_return,
        })
        .collect::<Vec<_>>();
    snapshot.sort_unstable();
    snapshot.dedup();
    let old = OBSERVATION_INTERESTS.swap(Box::into_raw(Box::new(snapshot)), Ordering::AcqRel);
    let mask = usize::from(entry_all) | (usize::from(return_all) << 1);
    OBSERVATION_ALL_MASK.store(mask, Ordering::Release);
    OBSERVATION_ENABLED.store(mask != 0 || !interests.is_empty(), Ordering::Release);
    if !old.is_null() {
        let mut retired = OBSERVATION_RETIRED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        retired.push(old as usize);
        if OBSERVATION_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<ObservationInterest>)) };
            }
        }
    }
}

struct ObservationReadGuard;

impl ObservationReadGuard {
    #[inline]
    fn new() -> Self {
        OBSERVATION_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ObservationReadGuard {
    #[inline]
    fn drop(&mut self) {
        OBSERVATION_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[inline]
pub fn observation_interested(address: u64, is_return: bool) -> bool {
    if !observation_enabled() {
        return false;
    }
    let bit = if is_return { 0b10 } else { 0b01 };
    if OBSERVATION_ALL_MASK.load(Ordering::Acquire) & bit != 0 {
        return true;
    }
    let _read_guard = ObservationReadGuard::new();
    let snapshot = OBSERVATION_INTERESTS.load(Ordering::Acquire);
    !snapshot.is_null()
        && unsafe { &*snapshot }
            .binary_search(&ObservationInterest { address, is_return })
            .is_ok()
}

fn lock_master() -> std::sync::MutexGuard<'static, Vec<u64>> {
    MASTER.lock().unwrap_or_else(|e| e.into_inner())
}

#[inline]
fn bloom_positions(address: u64) -> (usize, u64, usize, u64) {
    let mut first = address ^ (address >> 33);
    first = first.wrapping_mul(0xff51_afd7_ed55_8ccd);
    first ^= first >> 33;
    let mut second = address ^ (address >> 29);
    second = second.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    second ^= second >> 32;
    let first_bit = first as usize & (HOOK_BLOOM_BITS - 1);
    let second_bit = second as usize & (HOOK_BLOOM_BITS - 1);
    (
        first_bit >> 6,
        1u64 << (first_bit & 63),
        second_bit >> 6,
        1u64 << (second_bit & 63),
    )
}

fn bloom_insert(filter: &[AtomicU64; HOOK_BLOOM_WORDS], address: u64) {
    let (first_word, first_mask, second_word, second_mask) = bloom_positions(address);
    filter[first_word].fetch_or(first_mask, Ordering::Release);
    filter[second_word].fetch_or(second_mask, Ordering::Release);
}

#[inline]
fn bloom_maybe_contains(filter: &[AtomicU64; HOOK_BLOOM_WORDS], address: u64) -> bool {
    let (first_word, first_mask, second_word, second_mask) = bloom_positions(address);
    filter[first_word].load(Ordering::Acquire) & first_mask != 0
        && filter[second_word].load(Ordering::Acquire) & second_mask != 0
}

/// Publishes a fresh snapshot of `master` (query-server thread only).
fn publish(master: &[u64]) {
    let snapshot = Box::new(master.to_vec());
    let old = SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    COUNT.store(master.len(), Ordering::Release);
    if !old.is_null() {
        let mut retired = RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
        // contains() announces its read-side critical section before loading
        // the pointer. A quiescent publication can therefore reclaim every
        // old snapshot; an active/preempted instrumentation reader keeps the
        // retired copies alive until a later update. This avoids an O(n²)
        // memory leak when large DLL Hook sets are built incrementally.
        if SNAPSHOT_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<u64>)) };
            }
        }
    }
}

struct SnapshotReadGuard;

impl SnapshotReadGuard {
    #[inline]
    fn new() -> Self {
        SNAPSHOT_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for SnapshotReadGuard {
    #[inline]
    fn drop(&mut self) {
        SNAPSHOT_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Fast pre-check for on_ins: skip the pointer chase when no hooks exist.
#[inline]
pub fn any() -> bool {
    COUNT.load(Ordering::Acquire) > 0
}

/// Lock-free membership check for the instrumentation callback.
#[inline]
pub fn contains(address: u64) -> bool {
    if !bloom_maybe_contains(&HOOK_BLOOM, address) {
        return false;
    }
    let _read_guard = SnapshotReadGuard::new();
    let snapshot = SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return false;
    }
    unsafe { &*snapshot }.binary_search(&address).is_ok()
}

fn publish_functions(master: &[u64]) {
    let old = FUNCTIONS_SNAPSHOT.swap(Box::into_raw(Box::new(master.to_vec())), Ordering::AcqRel);
    FUNCTION_COUNT.store(master.len(), Ordering::Release);
    if !old.is_null() {
        let mut retired = FUNCTIONS_RETIRED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        retired.push(old as usize);
        if FUNCTIONS_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<u64>)) };
            }
        }
    }
}

struct FunctionReadGuard;

impl FunctionReadGuard {
    #[inline]
    fn new() -> Self {
        FUNCTIONS_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for FunctionReadGuard {
    #[inline]
    fn drop(&mut self) {
        FUNCTIONS_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Lock-free check used while Pin classifies a return instruction and again
/// on the native return callback. It never scans Hook rules or enters Python.
#[inline]
pub fn function_contains(address: u64) -> bool {
    if !bloom_maybe_contains(&FUNCTION_BLOOM, address) {
        return false;
    }
    let _read_guard = FunctionReadGuard::new();
    let snapshot = FUNCTIONS_SNAPSHOT.load(Ordering::Acquire);
    !snapshot.is_null() && unsafe { &*snapshot }.binary_search(&address).is_ok()
}

#[inline]
pub fn functions_any() -> bool {
    FUNCTION_COUNT.load(Ordering::Acquire) != 0
}

#[inline]
fn function_at_or_before(functions: &[u64], address: u64) -> Option<u64> {
    let index = functions.partition_point(|candidate| *candidate <= address);
    index
        .checked_sub(1)
        .and_then(|index| functions.get(index))
        .copied()
}

/// Resolves a selected export/function entry that owns a return instruction.
/// Pin may expose one coarse RTN whose start precedes several exported entry
/// points, so exact equality with `RTN_Address` is not sufficient.
pub fn function_for_return(
    return_instruction: u64,
    routine_start: u64,
    routine_end: u64,
) -> Option<u64> {
    if routine_end <= routine_start || return_instruction < routine_start {
        return None;
    }
    let _read_guard = FunctionReadGuard::new();
    let snapshot = FUNCTIONS_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return None;
    }
    let functions = unsafe { &*snapshot };
    let candidate = function_at_or_before(functions, return_instruction)?;
    (candidate >= routine_start && candidate < routine_end).then_some(candidate)
}

/// Fallback for images where Pin cannot map a return instruction back to a
/// useful RTN. The runtime function-call stack still validates the owner and
/// stack pointer, so a nested unselected function cannot produce a false
/// return event. Keep the lookup bounded to the same 64 KiB function window
/// used when publishing one-click function instrumentation.
pub fn function_for_return_near(return_instruction: u64, max_span: u64) -> Option<u64> {
    let _read_guard = FunctionReadGuard::new();
    let snapshot = FUNCTIONS_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return None;
    }
    let functions = unsafe { &*snapshot };
    let candidate = function_at_or_before(functions, return_instruction)?;
    (return_instruction.saturating_sub(candidate) <= max_span).then_some(candidate)
}

fn publish_signatures(master: &[FunctionSignatureLayout]) {
    let old = SIGNATURES_SNAPSHOT.swap(Box::into_raw(Box::new(master.to_vec())), Ordering::AcqRel);
    SIGNATURE_COUNT.store(master.len(), Ordering::Release);
    if !old.is_null() {
        let mut retired = SIGNATURES_RETIRED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        retired.push(old as usize);
        if SIGNATURES_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<FunctionSignatureLayout>)) };
            }
        }
    }
}

struct SignatureReadGuard;

impl SignatureReadGuard {
    #[inline]
    fn new() -> Self {
        SIGNATURES_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for SignatureReadGuard {
    #[inline]
    fn drop(&mut self) {
        SIGNATURES_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Lock-free exact-address lookup used only after a function Hook matched.
#[inline]
pub fn function_signature(address: u64) -> Option<FunctionSignatureLayout> {
    if SIGNATURE_COUNT.load(Ordering::Acquire) == 0 {
        return None;
    }
    let _read_guard = SignatureReadGuard::new();
    let snapshot = SIGNATURES_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return None;
    }
    let signatures = unsafe { &*snapshot };
    signatures
        .binary_search_by_key(&address, |signature| signature.address)
        .ok()
        .map(|index| signatures[index])
}

pub fn set_function_signature(mut layout: FunctionSignatureLayout) -> bool {
    if layout.address == 0
        || layout.parameter_count > 16
        || layout.float_parameter_mask & !0xffff != 0
        || layout.return_kind > VALUE_FLOAT64
        || layout.calling_convention > CALL_CONV_X86_FASTCALL
    {
        return false;
    }
    let parameter_mask = if layout.parameter_count == 0 {
        0
    } else {
        (1u32 << layout.parameter_count) - 1
    };
    layout.float_parameter_mask &= parameter_mask;
    let mut signatures = SIGNATURES_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match signatures.binary_search_by_key(&layout.address, |signature| signature.address) {
        Ok(index) => signatures[index] = layout,
        Err(_) if signatures.len() >= MAX_HOOK_POINTS => return false,
        Err(index) => signatures.insert(index, layout),
    }
    publish_signatures(&signatures);
    true
}

pub fn remove_function_signature(address: u64) {
    let mut signatures = SIGNATURES_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Ok(index) = signatures.binary_search_by_key(&address, |signature| signature.address) {
        signatures.remove(index);
        publish_signatures(&signatures);
    }
}

/// Forces re-JIT of one address so on_ins re-evaluates it.
fn flush(address: u64) {
    unsafe {
        pb_pin_remove_instrumentation_in_range(address, address + 15);
    }
}

/// Function-return capture can live well past the entry instruction. Batched
/// function logging therefore invalidates merged 64 KiB code intervals, not
/// just the first instruction. Typical DLL export clusters collapse to one
/// or a handful of Pin invalidations even for 20k entries.
fn flush_function_batch(addresses: &[u64]) {
    let Some(&first) = addresses.first() else {
        return;
    };
    const FUNCTION_WINDOW: u64 = 0xffff;
    let mut low = first;
    let mut high = first.saturating_add(FUNCTION_WINDOW);
    for &address in &addresses[1..] {
        let end = address.saturating_add(FUNCTION_WINDOW);
        if address <= high {
            high = high.max(end);
        } else {
            unsafe { pb_pin_remove_instrumentation_in_range(low, high) };
            low = address;
            high = end;
        }
    }
    unsafe { pb_pin_remove_instrumentation_in_range(low, high) };
}

/// Plain instruction batches invalidate only the small instruction windows
/// that contain newly inserted points. Adjacent windows are coalesced so a
/// range scan never performs one Pin invalidation per matched instruction.
fn flush_plain_batch(addresses: &[u64]) {
    let Some(&first) = addresses.first() else {
        return;
    };
    let mut low = first;
    let mut high = first.saturating_add(15);
    for &address in &addresses[1..] {
        let end = address.saturating_add(15);
        if address <= high.saturating_add(1) {
            high = high.max(end);
        } else {
            unsafe { pb_pin_remove_instrumentation_in_range(low, high) };
            low = address;
            high = end;
        }
    }
    unsafe { pb_pin_remove_instrumentation_in_range(low, high) };
}

/// Reclassifies existing Hook points between passive monitoring and writable
/// callback instrumentation after callback/rule interests change.
pub fn refresh_instrumentation(addresses: &[u64]) {
    let mut addresses = addresses
        .iter()
        .copied()
        .filter(|address| *address != 0)
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    flush_plain_batch(&addresses);
}

/// Callback-return interests attached to a function entry also change the
/// capture primitive at each decoded `ret` owned by that function.
pub fn refresh_callback_instrumentation(changes: &[(u64, bool)]) {
    let mut points = Vec::new();
    let mut functions = Vec::new();
    for &(address, is_return) in changes {
        if !contains(address) {
            continue;
        }
        if is_return && function_contains(address) {
            functions.push(address);
        } else {
            points.push(address);
        }
    }
    refresh_instrumentation(&points);
    functions.sort_unstable();
    functions.dedup();
    flush_function_batch(&functions);
}

/// Adds a hook point. Idempotent: an already-hooked address returns true.
/// Returns false when the set is full (MAX_HOOK_POINTS).
pub fn set(address: u64) -> bool {
    let mut master = lock_master();
    if master.binary_search(&address).is_ok() {
        return true;
    }
    if master.len() >= MAX_HOOK_POINTS {
        return false;
    }
    let index = master.binary_search(&address).unwrap_err();
    master.insert(index, address);
    bloom_insert(&HOOK_BLOOM, address);
    publish(&master);
    drop(master);
    flush(address);
    true
}

/// Adds a large address set with one sorted-snapshot publication and marks
/// every accepted address for function entry/return logging. Re-batching an
/// existing instruction Hook upgrades it to function logging even though the
/// returned `newly added` count remains zero.
pub fn set_batch(addresses: &[u64]) -> (usize, usize, bool) {
    let mut master = lock_master();
    let (merged, incoming, capacity_full) = merge_batch(&master, addresses, MAX_HOOK_POINTS);
    if !incoming.is_empty() {
        for &address in &incoming {
            bloom_insert(&HOOK_BLOOM, address);
        }
        *master = merged;
        publish(&master);
    }
    let total = master.len();
    let added = incoming.len();

    let mut accepted = addresses
        .iter()
        .copied()
        .filter(|address| *address != 0 && master.binary_search(address).is_ok())
        .collect::<Vec<_>>();
    accepted.sort_unstable();
    accepted.dedup();
    let mut functions = FUNCTIONS_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (merged_functions, new_functions, _) = merge_batch(&functions, &accepted, MAX_HOOK_POINTS);
    if !new_functions.is_empty() {
        for &address in &new_functions {
            bloom_insert(&FUNCTION_BLOOM, address);
        }
        *functions = merged_functions;
        publish_functions(&functions);
    }
    let mut invalidated = incoming;
    invalidated.extend_from_slice(&new_functions);
    invalidated.sort_unstable();
    invalidated.dedup();
    drop(functions);
    drop(master);
    flush_function_batch(&invalidated);
    (added, total, capacity_full)
}

/// Adds many ordinary instruction Hooks without upgrading them to function
/// entry/return logging. Used by range-based CALL/SYSCALL/branch selection.
pub fn set_plain_batch(addresses: &[u64]) -> (usize, usize, bool) {
    let mut master = lock_master();
    let (merged, incoming, capacity_full) = merge_batch(&master, addresses, MAX_HOOK_POINTS);
    if !incoming.is_empty() {
        for &address in &incoming {
            bloom_insert(&HOOK_BLOOM, address);
        }
        *master = merged;
        publish(&master);
    }
    let total = master.len();
    let added = incoming.len();
    drop(master);
    flush_plain_batch(&incoming);
    (added, total, capacity_full)
}

fn merge_batch(master: &[u64], addresses: &[u64], capacity: usize) -> (Vec<u64>, Vec<u64>, bool) {
    let mut incoming = addresses
        .iter()
        .copied()
        .filter(|address| *address != 0)
        .collect::<Vec<_>>();
    incoming.sort_unstable();
    incoming.dedup();

    incoming.retain(|address| master.binary_search(address).is_err());
    let available = capacity.saturating_sub(master.len());
    let capacity_full = incoming.len() > available;
    incoming.truncate(available);
    if incoming.is_empty() {
        return (master.to_vec(), incoming, capacity_full);
    }

    let mut merged = Vec::with_capacity(master.len() + incoming.len());
    let mut left = 0usize;
    let mut right = 0usize;
    while left < master.len() && right < incoming.len() {
        if master[left] < incoming[right] {
            merged.push(master[left]);
            left += 1;
        } else {
            merged.push(incoming[right]);
            right += 1;
        }
    }
    merged.extend_from_slice(&master[left..]);
    merged.extend_from_slice(&incoming[right..]);
    (merged, incoming, capacity_full)
}

/// Removes a hook point (no-op when absent).
pub fn remove(address: u64) {
    let mut master = lock_master();
    let mut changed = false;
    let mut function_changed = false;
    if let Ok(index) = master.binary_search(&address) {
        master.remove(index);
        publish(&master);
        changed = true;
    }
    let mut functions = FUNCTIONS_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Ok(index) = functions.binary_search(&address) {
        functions.remove(index);
        publish_functions(&functions);
        changed = true;
        function_changed = true;
    }
    drop(functions);
    drop(master);
    remove_function_signature(address);
    if function_changed {
        flush_function_batch(&[address]);
    } else if changed {
        flush(address);
    }
}

/// Removes all hook points, flushing each so stale capture calls die.
pub fn clear() {
    let mut master = lock_master();
    let addresses = master.clone();
    if !master.is_empty() {
        master.clear();
        publish(&master);
    }
    let mut functions = FUNCTIONS_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !functions.is_empty() {
        functions.clear();
        publish_functions(&functions);
    }
    drop(functions);
    drop(master);
    let mut signatures = SIGNATURES_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !signatures.is_empty() {
        signatures.clear();
        publish_signatures(&signatures);
    }
    drop(signatures);
    flush_function_batch(&addresses);
}

/// Current hook points, sorted.
pub fn list() -> Vec<u64> {
    lock_master().clone()
}

/// Function entries that emit both entry arguments and a normal-return event.
pub fn function_list() -> Vec<u64> {
    FUNCTIONS_MASTER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn publish_rules(rules: &[HookRule]) {
    let snapshot = Box::new(rules.to_vec());
    let old = RULES_SNAPSHOT.swap(Box::into_raw(snapshot), Ordering::AcqRel);
    RULE_COUNT.store(rules.len(), Ordering::Release);
    // Analysis callbacks announce a read-side critical section. Reclaim old
    // copies only at a quiescent publication; otherwise retain them until a
    // later update observes no readers.
    if !old.is_null() {
        let mut retired = RULES_RETIRED.lock().unwrap_or_else(|e| e.into_inner());
        retired.push(old as usize);
        if RULES_READERS.load(Ordering::SeqCst) == 0 {
            for address in retired.drain(..) {
                unsafe { drop(Box::from_raw(address as *mut Vec<HookRule>)) };
            }
        }
    }
}

struct RulesReadGuard;

impl RulesReadGuard {
    fn new() -> Self {
        RULES_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for RulesReadGuard {
    fn drop(&mut self) {
        RULES_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Instrumentation-time classification: true only when this address owns a
/// native action rule and therefore requires a writable Pin context.
#[inline]
pub fn rule_interested(address: u64) -> bool {
    if RULE_COUNT.load(Ordering::Acquire) == 0 {
        return false;
    }
    let _read_guard = RulesReadGuard::new();
    let snapshot = RULES_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() {
        return false;
    }
    let rules = unsafe { &*snapshot };
    let index = rules.partition_point(|item| item.address < address);
    index < rules.len() && rules[index].address == address
}

/// Adds or replaces one synchronous action rule. The hook address itself must
/// already be armed with `hook_set`/`hookall`. The first rule at an address
/// upgrades its passive monitor call to the writable callback path.
pub fn set_rule(rule: HookRule) -> bool {
    if rule.address == 0
        || rule.set_reg == PB_REG_INVALID_
        || (rule.match_reg != PB_REG_INVALID_ && rule.match_mask == 0)
    {
        return false;
    }
    let mut rules = RULES_MASTER.lock().unwrap_or_else(|e| e.into_inner());
    let same = |item: &HookRule| {
        item.address == rule.address
            && item.thread_id == rule.thread_id
            && item.match_reg == rule.match_reg
            && item.match_mask == rule.match_mask
            && item.match_value == rule.match_value
            && item.set_reg == rule.set_reg
    };
    let address_was_active = rules.iter().any(|item| item.address == rule.address);
    if let Some(existing) = rules.iter_mut().find(|item| same(item)) {
        existing.set_value = rule.set_value;
    } else {
        if rules.len() >= MAX_HOOK_RULES {
            return false;
        }
        rules.push(rule);
    }
    rules.sort_unstable_by_key(|item| item.address);
    publish_rules(&rules);
    drop(rules);
    if !address_was_active {
        refresh_instrumentation(&[rule.address]);
    }
    true
}

/// Removes all synchronous action rules. Hook points remain armed.
pub fn clear_rules() {
    let mut rules = RULES_MASTER.lock().unwrap_or_else(|e| e.into_inner());
    if rules.is_empty() {
        return;
    }
    let mut addresses = rules.iter().map(|rule| rule.address).collect::<Vec<_>>();
    rules.clear();
    publish_rules(&rules);
    drop(rules);
    addresses.sort_unstable();
    addresses.dedup();
    refresh_instrumentation(&addresses);
}

/// Allocates the small Pin TLS slot used for context-replay suppression.
/// Function-call pairing uses fixed tool storage so thread-start never waits
/// on the process heap while Pin owns its client lock.
pub fn init() -> PbStatus {
    let mut key = PB_INVALID_TLS_KEY;
    let status = unsafe { pb_pin_create_thread_data_key(None, &mut key) };
    if status != PB_OK {
        return status;
    }
    ACTION_TLS_KEY.store(key, Ordering::Release);
    status
}

#[inline]
unsafe fn function_call_state_slot(
    thread_id: PbThreadId,
) -> Option<&'static mut FunctionCallState> {
    let index = thread_id as usize;
    if index >= MAX_PIN_THREADS {
        return None;
    }
    let base = core::ptr::addr_of_mut!(FUNCTION_CALL_STATES) as *mut FunctionCallState;
    Some(&mut *base.add(index))
}

/// Low-frequency lifecycle initialization, called once for each application
/// thread before its first instrumented instruction executes. This path must
/// not allocate or acquire a process-heap lock.
pub unsafe fn thread_start(thread_id: PbThreadId) {
    if let Some(state) = function_call_state_slot(thread_id) {
        *state = FunctionCallState::new();
    }
}

pub unsafe fn thread_fini(thread_id: PbThreadId) {
    if let Some(state) = function_call_state_slot(thread_id) {
        *state = FunctionCallState::new();
    }
}

#[inline]
unsafe fn function_call_state(thread_id: PbThreadId) -> Option<&'static mut FunctionCallState> {
    function_call_state_slot(thread_id)
}

#[inline]
unsafe fn context_stack_pointer(context: PbContextHandle) -> Option<u64> {
    let mut stack_pointer = 0u64;
    (pb_pin_get_context_reg(
        context as PbConstContextHandle,
        crate::arch::stack_ptr_reg(),
        &mut stack_pointer,
    ) == PB_OK)
        .then_some(stack_pointer)
}

/// Pushes a selected function call frame. Stale frames left by longjmp,
/// exceptions, or tail calls are discarded when the stack has unwound back
/// to their level. Recursive calls have a lower stack pointer and remain.
pub unsafe fn track_function_entry(
    thread_id: PbThreadId,
    context: PbContextHandle,
    function_address: u64,
) {
    let Some(stack_pointer) = context_stack_pointer(context) else {
        return;
    };
    track_function_entry_stack(thread_id, stack_pointer, function_address);
}

/// Context-free form used by the passive monitoring ABI.
pub unsafe fn track_function_entry_stack(
    thread_id: PbThreadId,
    stack_pointer: u64,
    function_address: u64,
) {
    let Some(state) = function_call_state(thread_id) else {
        return;
    };
    while state.depth > 0 && state.frames[state.depth - 1].entry_stack_pointer <= stack_pointer {
        state.depth -= 1;
    }
    if state.depth == MAX_FUNCTION_CALL_DEPTH {
        return;
    }
    state.frames[state.depth] = FunctionCallFrame {
        function_address,
        entry_stack_pointer: stack_pointer,
    };
    state.depth += 1;
}

/// Matches a candidate `ret` to the selected function's live frame. Pin may
/// group stripped neighboring routines under one symbol; requiring both the
/// function entry and its entry stack pointer suppresses those false exits
/// and nested-call returns without a global call-stack scan.
pub unsafe fn take_function_return(
    thread_id: PbThreadId,
    context: PbContextHandle,
    function_address: u64,
) -> bool {
    let Some(stack_pointer) = context_stack_pointer(context) else {
        return false;
    };
    take_function_return_stack(thread_id, stack_pointer, function_address)
}

/// Context-free form used by the passive monitoring ABI.
pub unsafe fn take_function_return_stack(
    thread_id: PbThreadId,
    stack_pointer: u64,
    function_address: u64,
) -> bool {
    let Some(state) = function_call_state(thread_id) else {
        return false;
    };
    if state.depth == 0 {
        return false;
    }
    let frame = state.frames[state.depth - 1];
    if frame.function_address != function_address || frame.entry_stack_pointer != stack_pointer {
        return false;
    }
    state.depth -= 1;
    true
}

pub unsafe fn cancel_function_entry(thread_id: PbThreadId, function_address: u64) {
    let Some(state) = function_call_state(thread_id) else {
        return;
    };
    if state.depth != 0 && state.frames[state.depth - 1].function_address == function_address {
        state.depth -= 1;
    }
}

#[inline]
unsafe fn set_replay_guard(thread_id: PbThreadId, address: u64) -> bool {
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key == PB_INVALID_TLS_KEY {
        return false;
    }
    let encoded = address.checked_add(1).unwrap_or(address) as usize as *const c_void;
    let mut set = 0;
    let armed = pb_pin_set_thread_data(key, encoded, thread_id, &mut set) == PB_OK && set != 0;
    if armed {
        REPLAY_GUARD_COUNT.fetch_add(1, Ordering::Release);
    }
    armed
}

/// Consumes the guard left before `pb_pin_execute_at`; true means this is the
/// replayed instruction and its action must not be applied a second time.
pub unsafe fn take_replay_guard(thread_id: PbThreadId, address: u64) -> bool {
    if REPLAY_GUARD_COUNT.load(Ordering::Acquire) == 0 {
        return false;
    }
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key == PB_INVALID_TLS_KEY {
        return false;
    }
    let mut data = core::ptr::null_mut();
    if pb_pin_get_thread_data(key, thread_id, &mut data) != PB_OK || data.is_null() {
        return false;
    }
    let expected = address.checked_add(1).unwrap_or(address) as usize;
    if data as usize != expected {
        return false;
    }
    let mut cleared = 0;
    let _ = pb_pin_set_thread_data(key, core::ptr::null(), thread_id, &mut cleared);
    REPLAY_GUARD_COUNT.fetch_sub(1, Ordering::AcqRel);
    true
}

unsafe fn clear_replay_guard(thread_id: PbThreadId) {
    let key = ACTION_TLS_KEY.load(Ordering::Acquire);
    if key != PB_INVALID_TLS_KEY {
        let mut cleared = 0;
        if pb_pin_set_thread_data(key, core::ptr::null(), thread_id, &mut cleared) == PB_OK
            && cleared != 0
        {
            REPLAY_GUARD_COUNT.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// Applies all matching rules in the current Pin analysis callback. Returns
/// how many context writes succeeded. No locks, allocation, I/O, or Python
/// calls occur on this hot path.
pub unsafe fn apply_rules(
    address: u64,
    thread_id: PbThreadId,
    context: PbContextHandle,
    captured_args: [u64; 4],
    is_return_instruction: bool,
) -> u32 {
    if RULE_COUNT.load(Ordering::Acquire) == 0 {
        return 0;
    }
    let _read_guard = RulesReadGuard::new();
    let snapshot = RULES_SNAPSHOT.load(Ordering::Acquire);
    if snapshot.is_null() || context.is_null() {
        return 0;
    }
    let rules = &*snapshot;
    let mut index = rules.partition_point(|item| item.address < address);
    let mut writes = 0;
    while index < rules.len() && rules[index].address == address {
        let rule = rules[index];
        index += 1;
        if rule.thread_id != PB_INVALID_THREAD_ID && rule.thread_id != thread_id {
            continue;
        }
        // ABI stack offsets describe function-entry layout. A return hook is
        // never at function entry, so applying a stack rule there could read
        // or overwrite an unrelated caller slot.
        if is_return_instruction
            && (stack_arg_index(rule.match_reg).is_some()
                || stack_arg_index(rule.set_reg).is_some())
        {
            continue;
        }
        if rule.match_reg != PB_REG_INVALID_ {
            let current = crate::arch::hook_arg_regs()
                .iter()
                .position(|reg| *reg == rule.match_reg)
                .map(|index| captured_args[index])
                .or_else(|| {
                    if let Some(index) = stack_arg_index(rule.match_reg) {
                        let mut value = 0;
                        return (pb_pin_get_context_stack_arg(
                            context as PbConstContextHandle,
                            index,
                            &mut value,
                        ) == PB_OK)
                            .then_some(value);
                    }
                    let mut value = 0;
                    (pb_pin_get_context_reg(
                        context as PbConstContextHandle,
                        rule.match_reg,
                        &mut value,
                    ) == PB_OK)
                        .then_some(value)
                });
            if current
                .map(|value| (value & rule.match_mask) == (rule.match_value & rule.match_mask))
                != Some(true)
            {
                continue;
            }
        }
        let applied = if let Some(index) = stack_arg_index(rule.set_reg) {
            pb_pin_set_context_stack_arg(context, index, rule.set_value) == PB_OK
        } else {
            pb_pin_set_context_reg(context, rule.set_reg, rule.set_value) == PB_OK
        };
        if applied {
            writes += 1;
        }
    }
    writes
}

/// Marks the current thread for one callback replay and transfers control to
/// the modified context. Returns false if the TLS guard could not be armed.
pub unsafe fn execute_modified_context(
    thread_id: PbThreadId,
    address: u64,
    context: PbContextHandle,
) -> bool {
    if !set_replay_guard(thread_id, address) {
        return false;
    }
    let status = pb_pin_execute_at(context as PbConstContextHandle);
    if status != PB_OK {
        clear_replay_guard(thread_id);
        false
    } else {
        true
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[test]
    fn finds_nearest_selected_function_at_or_before_return() {
        let functions = [0x1000, 0x1100, 0x3000];
        assert_eq!(function_at_or_before(&functions, 0x0fff), None);
        assert_eq!(function_at_or_before(&functions, 0x1000), Some(0x1000));
        assert_eq!(function_at_or_before(&functions, 0x10ff), Some(0x1000));
        assert_eq!(function_at_or_before(&functions, 0x1100), Some(0x1100));
        assert_eq!(function_at_or_before(&functions, 0x4000), Some(0x3000));
    }

    #[test]
    fn merges_twenty_thousand_hooks_sorted_and_deduplicated() {
        let addresses = (0..20_000u64)
            .rev()
            .flat_map(|index| [0x1800_0000 + index * 16, 0x1800_0000 + index * 16])
            .collect::<Vec<_>>();
        let (merged, added, full) = merge_batch(&[], &addresses, MAX_HOOK_POINTS);
        assert!(!full);
        assert_eq!(added.len(), 20_000);
        assert_eq!(merged.len(), 20_000);
        assert!(merged.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(merged.binary_search(&0x1800_0000).is_ok());
        assert!(merged.binary_search(&(0x1800_0000 + 19_999 * 16)).is_ok());
    }

    #[test]
    fn batch_preserves_existing_points_and_reports_capacity() {
        let master = vec![0x1000, 0x3000];
        let (merged, added, full) = merge_batch(&master, &[0x1000, 0x2000, 0x4000, 0x5000], 4);
        assert!(full);
        assert_eq!(added, vec![0x2000, 0x4000]);
        assert_eq!(merged, vec![0x1000, 0x2000, 0x3000, 0x4000]);
    }

    #[test]
    fn bloom_prefilter_never_rejects_inserted_hook_addresses() {
        for address in (0..32_768u64).map(|index| 0x1800_0000 + index * 17) {
            bloom_insert(&HOOK_BLOOM, address);
            assert!(bloom_maybe_contains(&HOOK_BLOOM, address));
        }
    }
}
