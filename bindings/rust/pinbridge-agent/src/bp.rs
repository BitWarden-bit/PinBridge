//! Instrumentation breakpoints (no 0xCC, no hardware DR): when Pin JITs an
//! instruction whose address is in the table, an analysis call (with the
//! thread context) is inserted. On a hit the thread is redirected into the
//! architecture-specific x64/ia32 park stub via execute_at *before* the bp
//! instruction runs, the breaker
//! stops the application, and the stopped context is rewound onto the bp
//! address — the stop is exact and side-effect-free. Set/remove flush the
//! address range to force re-JIT.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

const MAX_BREAKPOINTS: usize = 64;

struct Slot {
    used: bool,
    id: u32,
    address: u64,
    hits: AtomicU64,
    active: AtomicBool,
    one_shot: AtomicBool,
}

impl Slot {
    const fn empty() -> Slot {
        Slot {
            used: false,
            id: 0,
            address: 0,
            hits: AtomicU64::new(0),
            active: AtomicBool::new(false),
            one_shot: AtomicBool::new(false),
        }
    }
}

static mut SLOTS: [Slot; MAX_BREAKPOINTS] = [const { Slot::empty() }; MAX_BREAKPOINTS];
static TABLE_MUTEX: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

/// Control channel: Pin pairs stop/resume with the *calling* internal thread,
/// so ALL stop/resume must go through the breaker thread (single owner).
/// A resume issued by any other thread kills the process (pinvm assert).
pub static CMD_STOP: u32 = 1;
pub static CMD_RESUME: u32 = 2;
static CMD: AtomicU32 = AtomicU32::new(0);
static CMD_RESULT: AtomicU32 = AtomicU32::new(0);
static CMD_SEM: AtomicUsize = AtomicUsize::new(0);
static ACK_SEM: AtomicUsize = AtomicUsize::new(0);
static HIT_PENDING: AtomicBool = AtomicBool::new(false);

/// Step watchdog: armed when a step into/over is issued (in ~50ms breaker
/// ticks). If the landing never arrives — a step-over whose call blocks or
/// never returns — the breaker auto-pauses the app and hands control back
/// instead of silently running off. Cleared by every completed stop.
static WATCHDOG_TICKS: AtomicU32 = AtomicU32::new(0);

/// Arm the step watchdog for ~`ticks` breaker iterations (~50ms each).
pub fn arm_step_watchdog(ticks: u32) {
    WATCHDOG_TICKS.store(ticks, Ordering::Release);
}

/// Stop generation: bumped by the breaker on every completed stop (bp hit,
/// step landing, manual pause). Polling clients MUST key refreshes off this
/// counter, not off running->stopped edges — a stop/run/stop cycle can
/// complete inside one poll window and the edge is then invisible.
static STOP_GEN: AtomicU64 = AtomicU64::new(0);

pub fn stop_gen() -> u64 {
    STOP_GEN.load(Ordering::Acquire)
}

/// No-hit sentinel: Pin thread ids start at 0 (tid 0 is the main thread),
/// so "none" must be u32::MAX, never 0.
pub const NO_HIT_TID: u32 = u32::MAX;

/// Last breakpoint hit that stopped the app: (thread id, address). The UI
/// uses it to select the right thread instead of guessing threads[0].
/// Cleared to NO_HIT_TID on every successful resume so a manual pause
/// doesn't inherit a stale hit.
static LAST_HIT_TID: AtomicU32 = AtomicU32::new(NO_HIT_TID);
static LAST_HIT_ADDR: AtomicU64 = AtomicU64::new(0);

/// (tid, address) of the hit that caused the current stop; (NO_HIT_TID, 0)
/// if the stop came from a manual pause or the info was consumed by a resume.
pub fn last_hit() -> (u32, u64) {
    (
        LAST_HIT_TID.load(Ordering::Acquire),
        LAST_HIT_ADDR.load(Ordering::Acquire),
    )
}

/// Exact-stop park stub (x86-64, 24 bytes in a RWX VirtualAlloc page):
///   L0: cmp byte [rip+8], 0   ; flag at L0+15
///       je L0                 ; spin while no exit requested
///       jmp qword [rip+1]     ; target slot at L0+16 (safety exit)
///   flag: db 0   target: dq
/// A breakpoint hit redirects the hitting thread HERE via execute_at,
/// *before* the bp instruction runs: the thread spins in JITted code (a
/// valid safe point, unlike a thread blocked inside an analysis call), so
/// stop_application_threads suspends it with the bp instruction still
/// un-executed. The breaker then rewinds the stopped context rip to the bp
/// address — the stop is exact AND side-effect-free, and resume replays the
/// hit once (swallowed by the resume-skip above).
static STUB_BASE: AtomicUsize = AtomicUsize::new(0);
static STUB_CODE: [u8; 15] = [
    0x80, 0x3D, 0x08, 0x00, 0x00, 0x00, 0x00, // cmp byte [rip+8], 0
    0x74, 0xF7, // je -9 (back to start)
    0xFF, 0x25, 0x01, 0x00, 0x00, 0x00, // jmp qword [rip+1]
];
/// ia32 equivalent (20 bytes total after flag + u32 target):
///   L0: cmp byte [absolute flag], 0
///       je L0
///       jmp dword [absolute target slot]
/// The two absolute addresses are patched after VirtualAlloc, before the
/// page is published to analysis callbacks.
static STUB_CODE_X86: [u8; 15] = [
    0x80, 0x3D, 0, 0, 0, 0, 0x00, // cmp byte [flag], 0
    0x74, 0xF7, // je -9 (back to start)
    0xFF, 0x25, 0, 0, 0, 0, // jmp dword [target slot]
];
const STUB_FLAG_OFF: usize = 15;
const STUB_TARGET_OFF: usize = 16;

extern "system" {
    fn VirtualAlloc(addr: *mut c_void, size: usize, ty: u32, protect: u32) -> *mut c_void;
}

static REDIRECT_ACTIVE: AtomicBool = AtomicBool::new(false);
static REDIRECT_TID: AtomicU32 = AtomicU32::new(0);
static REDIRECT_ADDR: AtomicU64 = AtomicU64::new(0);

fn init_stub() {
    // MEM_COMMIT|MEM_RESERVE, PAGE_EXECUTE_READWRITE
    let page = unsafe { VirtualAlloc(core::ptr::null_mut(), 4096, 0x3000, 0x40) };
    if page.is_null() {
        crate::log::line("park stub alloc failed; stops will drift");
        return;
    }
    unsafe {
        if crate::arch::is_64() {
            core::ptr::copy_nonoverlapping(STUB_CODE.as_ptr(), page as *mut u8, STUB_CODE.len());
        } else {
            core::ptr::copy_nonoverlapping(
                STUB_CODE_X86.as_ptr(),
                page as *mut u8,
                STUB_CODE_X86.len(),
            );
            let base = page as usize;
            // x86 addresses are 32-bit by construction; write_unaligned
            // because the immediate fields begin at byte 2 and byte 11.
            core::ptr::write_unaligned((base + 2) as *mut u32, (base + STUB_FLAG_OFF) as u32);
            core::ptr::write_unaligned((base + 11) as *mut u32, (base + STUB_TARGET_OFF) as u32);
        }
    }
    STUB_BASE.store(page as usize, Ordering::Release);
    crate::log::line(&format!(
        "{} park stub at {page:p}",
        if crate::arch::is_64() { "x64" } else { "x86" }
    ));
}

/// Claims the single park-stub slot for (tid, address). False when another
/// redirect is in flight — the caller falls back to a plain request_stop()
/// and the in-flight stop catches the thread wherever it is.
fn redirect_arm(tid: u32, address: u64) -> bool {
    let stub = STUB_BASE.load(Ordering::Acquire);
    if stub == 0 {
        return false;
    }
    if REDIRECT_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    REDIRECT_TID.store(tid, Ordering::Release);
    REDIRECT_ADDR.store(address, Ordering::Release);
    unsafe {
        if crate::arch::is_64() {
            core::ptr::write_volatile((stub + STUB_TARGET_OFF) as *mut u64, address);
        } else {
            let Ok(address) = u32::try_from(address) else {
                REDIRECT_ACTIVE.store(false, Ordering::Release);
                return false;
            };
            core::ptr::write_volatile((stub + STUB_TARGET_OFF) as *mut u32, address);
        }
        core::ptr::write_volatile((stub + STUB_FLAG_OFF) as *mut u8, 0);
    }
    true
}

/// Breaker side, right after the world stopped: roll the redirected thread's
/// saved context back to the breakpoint address. Safe because the bp
/// instruction never executed (the thread was redirected pre-execution).
unsafe fn rewind_redirected() {
    if !REDIRECT_ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let tid = REDIRECT_TID.load(Ordering::Acquire);
    let address = REDIRECT_ADDR.load(Ordering::Acquire);
    let mut context: PbContextHandle = core::ptr::null_mut();
    if pb_pin_get_stopped_thread_writeable_context(tid, &mut context) == PB_OK && !context.is_null()
    {
        pb_pin_set_context_reg(context, crate::arch::instr_ptr_reg(), address);
    }
}

/// Breaker side, before resuming: free the stub slot; any straggler that
/// entered the spin after the sweep exits via the target slot.
unsafe fn release_redirect() {
    if REDIRECT_ACTIVE.swap(false, Ordering::AcqRel) {
        let stub = STUB_BASE.load(Ordering::Acquire);
        if stub != 0 {
            core::ptr::write_volatile((stub + STUB_FLAG_OFF) as *mut u8, 1);
        }
    }
}

/// Breaker post-stop: private successor callbacks are analysis-time gated, so
/// cancellation is enough and can never delete a normal breakpoint.
fn clear_step_bps() {
    crate::stepper::cancel();
}

/// Resume-over-breakpoint replay suppression. The hitting thread is stopped
/// *before* the breakpoint instruction, so a plain resume re-executes the
/// instrumented call and would instantly re-stop forever (the 0xCC world
/// solves this by restoring the byte and stepping over; we cannot). Instead
/// the resume path arms this pair, and the hitting thread's FIRST on_hit
/// after the resume swallows exactly that one re-execution. Consumed on the
/// thread's first hit even at a different address (the replay window ends
/// with any hit), and re-armed from last_hit() on every resume.
static RESUME_SKIP_TID: AtomicU32 = AtomicU32::new(NO_HIT_TID);
static RESUME_SKIP_ADDR: AtomicU64 = AtomicU64::new(0);

/// rip of a stopped thread, if the runtime can provide it. Called from the
/// query-server thread while the application is stopped.
fn stopped_thread_rip(tid: u32) -> Option<u64> {
    unsafe {
        let mut context: PbConstContextHandle = core::ptr::null();
        if pb_pin_get_stopped_thread_context(tid, &mut context) != PB_OK || context.is_null() {
            return None;
        }
        let mut rip: u64 = 0;
        pb_pin_get_context_reg(context, crate::arch::instr_ptr_reg(), &mut rip);
        Some(rip)
    }
}

/// Arms replay suppression from the hit that caused the current stop.
/// Call right before a plain resume (steps use the stepper's own replay
/// suppression and must not arm this). NO_HIT_TID disarms (manual pause).
///
/// Only arms when the hit thread is still positioned AT the breakpoint
/// (rip == hit address, i.e. suspended pre-execution): only then will a
/// plain resume re-execute the instrumented call. Park-on-hit stops leave
/// the thread a few instructions PAST the breakpoint (it executed during the
/// release->suspend window), no replay can occur, and arming would wrongly
/// swallow the next genuine hit.
pub fn arm_resume_skip() {
    let (tid, address) = last_hit();
    let arm = tid != NO_HIT_TID && stopped_thread_rip(tid) == Some(address);
    if arm {
        RESUME_SKIP_ADDR.store(address, Ordering::Release);
        RESUME_SKIP_TID.store(tid, Ordering::Release);
    } else {
        RESUME_SKIP_TID.store(NO_HIT_TID, Ordering::Release);
    }
}

/// Fast pre-check used at instrumentation time (no lock when table empty).
pub fn any_active() -> bool {
    ACTIVE_COUNT.load(Ordering::Relaxed) > 0
}

fn lock_table() -> bool {
    let mutex = TABLE_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    !mutex.is_null() && unsafe { pb_pin_mutex_lock(mutex) == PB_OK }
}

fn unlock_table() {
    let mutex = TABLE_MUTEX.load(Ordering::Acquire) as PbMutexHandle;
    if !mutex.is_null() {
        unsafe {
            pb_pin_mutex_unlock(mutex);
        }
    }
}

/// Slot index of the active breakpoint at `address`, if any.
pub fn find(address: u64) -> Option<usize> {
    if !any_active() || !lock_table() {
        return None;
    }
    let mut found = None;
    unsafe {
        for (index, slot) in (*core::ptr::addr_of!(SLOTS)).iter().enumerate() {
            if slot.used && slot.address == address && slot.active.load(Ordering::Relaxed) {
                found = Some(index);
                break;
            }
        }
    }
    unlock_table();
    found
}

/// Called from the instrumentation callback for a breakpoint address.
pub fn instrument(ins: PbInsHandle, slot_index: usize) {
    // NOTE: never log here — this is an instrumentation callback running on
    // an application thread; a std-mutex/file-I/O log can be held exactly
    // when the breaker suspends the thread and deadlocks the control plane.
    unsafe {
        pb_ins_insert_call_before_ctx(ins, Some(on_hit_ctx), slot_index as *mut c_void);
    }
}

/// Called when a decoded step successor has no ordinary breakpoint at the
/// same address. `candidate_index` refers to the stepper's private table.
pub fn instrument_step(ins: PbInsHandle, candidate_index: usize) {
    unsafe {
        pb_ins_insert_call_before_ctx(ins, Some(on_step_hit_ctx), candidate_index as *mut c_void);
    }
}

pub(crate) unsafe fn exact_stop(context: PbContextHandle, tid: u32, address: u64) {
    LAST_HIT_TID.store(tid, Ordering::Release);
    LAST_HIT_ADDR.store(address, Ordering::Release);
    let armed = !context.is_null() && redirect_arm(tid, address);
    if armed {
        request_stop();
        let status = pb_pin_set_context_reg(
            context,
            crate::arch::instr_ptr_reg(),
            STUB_BASE.load(Ordering::Acquire) as u64,
        );
        if status == PB_OK {
            pb_pin_execute_at(context as PbConstContextHandle); // noreturn
        } else {
            release_redirect();
        }
    }
    request_stop();
}

unsafe extern "C" fn on_step_hit_ctx(context: PbContextHandle, user_data: *mut c_void) {
    let candidate_index = user_data as usize;
    let mut tid: PbThreadId = 0;
    if pb_pin_thread_id(&mut tid) != PB_OK {
        return;
    }
    let address = crate::stepper::candidate_address(candidate_index);
    if address != 0 && crate::stepper::consume_start_replay(tid as u32, address) {
        return;
    }
    let Some(address) = crate::stepper::claim_candidate(candidate_index, tid as u32) else {
        return;
    };
    exact_stop(context, tid as u32, address);
}

/// Analysis callback (with thread context): fires on every execution of a
/// breakpoint instruction.
unsafe extern "C" fn on_hit_ctx(context: PbContextHandle, user_data: *mut c_void) {
    let index = user_data as usize;
    if index >= MAX_BREAKPOINTS {
        return;
    }
    let slots = &*core::ptr::addr_of!(SLOTS);
    let slot = &slots[index];
    if !slot.active.load(Ordering::Relaxed) {
        return; // removed but not yet re-JITed away
    }
    // Resume-over-breakpoint replay suppression: swallow exactly the first
    // re-execution on the thread that caused the stop (see arm_resume_skip).
    // NOTE: analysis callback on an application thread — no logging, no std
    // locks, no I/O in here (see instrument()).
    let mut tid: PbThreadId = 0;
    let have_tid = pb_pin_thread_id(&mut tid) == PB_OK;
    if have_tid && crate::stepper::consume_start_replay(tid as u32, slot.address) {
        return;
    }
    let skip_tid = RESUME_SKIP_TID.load(Ordering::Acquire);
    if skip_tid != NO_HIT_TID && have_tid && skip_tid == tid as u32 {
        RESUME_SKIP_TID.store(NO_HIT_TID, Ordering::Release); // window closed
        if RESUME_SKIP_ADDR.load(Ordering::Acquire) == slot.address {
            return;
        }
    }
    slot.hits.fetch_add(1, Ordering::Relaxed);
    if slot.one_shot.load(Ordering::Relaxed) {
        slot.active.store(false, Ordering::Relaxed);
        ACTIVE_COUNT.fetch_sub(1, Ordering::Relaxed);
    }
    // Record who hit it before asking the breaker to stop the world.
    // PIN_ThreadId is one of the few calls legal in analysis code.
    if have_tid {
        // An overlapping normal breakpoint both completes the step and keeps
        // its own independent lifetime/hit accounting.
        let _ = crate::stepper::claim_breakpoint(tid as u32, slot.address);
        exact_stop(context, tid as u32, slot.address);
        return;
    }
    LAST_HIT_TID.store(NO_HIT_TID, Ordering::Release);
    LAST_HIT_ADDR.store(slot.address, Ordering::Release);
    // Without a Pin thread id we cannot bind this stop to a writable context;
    // retain the safe inexact fallback and let the breaker stop the world.
    request_stop();
}

/// Breaker-side hold-off for the one-time python310.dll load: stopping the
/// application while the scripting thread is inside LoadLibraryExW wedges
/// the process (the load never finishes while the app is stopped, the OS
/// loader lock stays held, and the query server's next first-called
/// delay-loaded synch import blocks behind it — the observed stress wedge).
/// Wait for the load window to close; BOUNDED so a genuinely stuck loader
/// can never disable stops (~6s, then proceed regardless).
fn wait_loader_quiesce() {
    if !crate::scripting::py_load_in_flight() {
        return;
    }
    crate::log::line("breaker: stop deferred during python load");
    for _ in 0..600 {
        if !crate::scripting::py_load_in_flight() {
            return;
        }
        unsafe {
            pb_pin_sleep(10);
        }
    }
    crate::log::line("breaker: python load still in flight after 6s; stopping anyway");
}

/// Breaker internal thread: the single owner of stop/resume.
unsafe extern "C" fn breaker_main(_argument: *mut c_void) {
    let cmd_sem = CMD_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    let ack_sem = ACK_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    let mut tid: PbThreadId = 0;
    if pb_pin_thread_id(&mut tid) != PB_OK {
        return;
    }
    crate::log::line(&format!(
        "breaker thread up (pin tid {tid}, os tid {})",
        crate::diag::os_tid()
    ));
    let mut iterations: u32 = 0;
    loop {
        let mut woke: u8 = 0;
        if pb_pin_semaphore_timed_wait(cmd_sem, 50, &mut woke) != PB_OK {
            return;
        }
        // PIN semaphores are binary state, not counting: nothing auto-clears
        // on wake. cmd_sem must be cleared here or the first pulse leaves it
        // set forever and this loop busy-spins at 100% CPU (observed: 1.5M
        // iterations/s, starving pinvm of a core for the whole session).
        // A pulse landing between the clear and the next wait stays set, so
        // no wakeup is lost.
        pb_pin_semaphore_clear(cmd_sem);
        iterations += 1;
        if iterations % 100 == 0 {
            crate::diag::heap_check("breaker");
        }

        // step watchdog: a step whose landing never arrives would otherwise
        // leave the app running off with a stray one-shot bp armed
        let ticks = WATCHDOG_TICKS.load(Ordering::Acquire);
        if ticks > 0 {
            let next = ticks - 1;
            WATCHDOG_TICKS.store(next, Ordering::Release);
            if next == 0 && !crate::control::is_stopped() {
                crate::log::line("step watchdog: landing never came, auto-pause");
                HIT_PENDING.store(true, Ordering::Release);
            }
        }

        // breakpoint hit / step landing -> stop
        if HIT_PENDING.swap(false, Ordering::AcqRel) {
            crate::log::line("breaker woke");
            // A thread parked by the stepper/breakpoint is NOT a safe point
            // for stop_application_threads (it waits forever and deadlocks
            // us): begin_suspend blocks new parks and releases the parked
            // thread so it can run to the suspension point.
            crate::stepper::begin_suspend();
            if !crate::control::is_stopped() {
                wait_loader_quiesce();
                let mut stopped: u8 = 0;
                if pb_pin_stop_application_threads(tid, &mut stopped) == PB_OK && stopped != 0 {
                    WATCHDOG_TICKS.store(0, Ordering::Release);
                    // with the world frozen, roll the redirected thread's
                    // saved context back onto the breakpoint address
                    rewind_redirected();
                    clear_step_bps();
                    // Publish the stop LAST: observers (script on_stop, UI
                    // polls) must never see stopped=true while the breaker is
                    // still mutating stopped contexts — a get_context racing
                    // the rewind crashes inside Pin's VM.
                    crate::control::STOPPED.store(true, Ordering::Release);
                    let stop_generation = STOP_GEN.fetch_add(1, Ordering::AcqRel) + 1;
                    crate::execution_trap::publish_stopped(stop_generation);
                }
            }
        }

        // control command -> stop/resume
        let cmd = CMD.swap(0, Ordering::AcqRel);
        if cmd != 0 {
            let ok = if cmd == CMD_STOP {
                crate::stepper::begin_suspend(); // no new parks; free parked
                wait_loader_quiesce();
                let mut stopped: u8 = 0;
                let status = pb_pin_stop_application_threads(tid, &mut stopped);
                let good = status == PB_OK && stopped != 0;
                if good {
                    WATCHDOG_TICKS.store(0, Ordering::Release);
                    rewind_redirected();
                    clear_step_bps();
                    // publish last (see above)
                    crate::control::STOPPED.store(true, Ordering::Release);
                    let stop_generation = STOP_GEN.fetch_add(1, Ordering::AcqRel) + 1;
                    crate::execution_trap::publish_stopped(stop_generation);
                }
                good
            } else {
                crate::stepper::release(); // free the parked thread, then unsuspend
                release_redirect(); // stragglers exit the spin via the target slot
                let status = pb_pin_resume_application_threads(tid);
                let good = status == PB_OK;
                if good {
                    crate::control::STOPPED.store(false, Ordering::Release);
                    crate::stepper::end_suspend();
                    crate::execution_trap::on_resume();
                    LAST_HIT_TID.store(NO_HIT_TID, Ordering::Release);
                    LAST_HIT_ADDR.store(0, Ordering::Release);
                }
                good
            };
            crate::log::line(&format!("breaker cmd={cmd} ok={ok}"));
            CMD_RESULT.store(ok as u32, Ordering::Release);
            pb_pin_semaphore_set(ack_sem);
        }
    }
}

/// Serializes control_command callers: the query-server thread AND the
/// script host thread both send stop/resume through here, and the static
/// CMD/CMD_RESULT pair would race otherwise. Never taken in analysis
/// callbacks (breaker acts on CMD directly), so a std mutex is safe.
static CONTROL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Sends a stop/resume command to the breaker and waits for the result.
/// Returns false when the breaker is unavailable or the action failed.
pub fn control_command(cmd: u32) -> bool {
    let _guard = CONTROL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cmd_sem = CMD_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    let ack_sem = ACK_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    if cmd_sem.is_null() || ack_sem.is_null() {
        return false;
    }
    unsafe {
        CMD.store(cmd, Ordering::Release);
        pb_pin_semaphore_clear(ack_sem);
        pb_pin_semaphore_set(cmd_sem);
        let mut woke: u8 = 0;
        if pb_pin_semaphore_timed_wait(ack_sem, 10_000, &mut woke) != PB_OK || woke == 0 {
            return false;
        }
        CMD_RESULT.load(Ordering::Acquire) != 0
    }
}

pub fn init() -> PbStatus {
    init_stub();
    unsafe {
        let mut mutex: PbMutexHandle = core::ptr::null_mut();
        let status = pb_pin_mutex_init(&mut mutex);
        if status != PB_OK {
            return status;
        }
        TABLE_MUTEX.store(mutex as usize, Ordering::Release);

        for target in [&CMD_SEM, &ACK_SEM] {
            let mut semaphore: PbSemaphoreHandle = core::ptr::null_mut();
            let status = pb_pin_semaphore_init(&mut semaphore);
            if status != PB_OK {
                return status;
            }
            target.store(semaphore as usize, Ordering::Release);
        }

        let mut tid: PbThreadId = 0;
        let mut uid: PbPinThreadUid = 0;
        pb_pin_spawn_internal_thread(
            Some(breaker_main),
            core::ptr::null_mut(),
            0,
            &mut tid,
            &mut uid,
        )
    }
}

/// Asks the breaker to stop the application (used by breakpoints and the
/// exception policy). Sets the flag and pulses the breaker's semaphore so it
/// wakes immediately instead of after its 50ms poll; the pulse is
/// best-effort (the poll is the fallback).
pub fn request_stop() {
    HIT_PENDING.store(true, Ordering::Release);
    let cmd_sem = CMD_SEM.load(Ordering::Acquire) as PbSemaphoreHandle;
    if !cmd_sem.is_null() {
        unsafe {
            pb_pin_semaphore_set(cmd_sem);
        }
    }
}

/// Registers a breakpoint and flushes the address so it gets re-JITed.
pub fn set(address: u64) -> Result<u32, PbStatus> {
    set_impl(address, false)
}

/// One-shot breakpoint: auto-deactivates on the first hit (step-over landing).
pub fn set_oneshot(address: u64) -> Result<u32, PbStatus> {
    set_impl(address, true)
}

fn set_impl(address: u64, one_shot: bool) -> Result<u32, PbStatus> {
    if address == 0 {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }
    if !lock_table() {
        return Err(PB_ERR_INVALID_STATE);
    }
    let mut result = Err(PB_ERR_OUT_OF_MEMORY);
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        // same address reuses the slot
        if let Some(slot) = slots
            .iter()
            .find(|s| s.used && s.address == address && s.active.load(Ordering::Relaxed))
        {
            result = Ok(slot.id);
        } else if let Some(slot) = slots
            .iter_mut()
            .find(|s| !s.used || !s.active.load(Ordering::Relaxed))
        {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) as u32;
            slot.used = true;
            slot.id = id;
            slot.address = address;
            slot.hits.store(0, Ordering::Relaxed);
            slot.one_shot.store(one_shot, Ordering::Relaxed);
            slot.active.store(true, Ordering::Relaxed);
            ACTIVE_COUNT.fetch_add(1, Ordering::Relaxed);
            result = Ok(id);
        }
    }
    unlock_table();
    if result.is_ok() {
        unsafe {
            let status = pb_pin_remove_instrumentation_in_range(address, address + 15);
            crate::log::line(&format!("bp flush 0x{address:x} -> {status}"));
        }
    }
    result
}

pub fn remove(id: u32) -> bool {
    if !lock_table() {
        return false;
    }
    let mut removed = false;
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        if let Some(slot) = slots.iter_mut().find(|s| s.used && s.id == id) {
            slot.active.store(false, Ordering::Relaxed);
            slot.used = false;
            ACTIVE_COUNT.fetch_sub(1, Ordering::Relaxed);
            removed = true;
            pb_pin_remove_instrumentation_in_range(slot.address, slot.address + 15);
        }
    }
    unlock_table();
    removed
}

/// (id, address, hits) for all live breakpoints. The Vec is reserved BEFORE
/// taking the table mutex: allocating under a Pin mutex is an AB-BA deadlock
/// vector against threads that hold the process-heap lock while blocking on
/// Pin locks (see ring.rs).
pub fn list() -> Vec<(u32, u64, u64)> {
    let mut out = Vec::with_capacity(MAX_BREAKPOINTS);
    if !lock_table() {
        return out;
    }
    unsafe {
        for slot in (*core::ptr::addr_of!(SLOTS)).iter() {
            if slot.used && slot.active.load(Ordering::Relaxed) {
                out.push((slot.id, slot.address, slot.hits.load(Ordering::Relaxed)));
            }
        }
    }
    unlock_table();
    out
}
