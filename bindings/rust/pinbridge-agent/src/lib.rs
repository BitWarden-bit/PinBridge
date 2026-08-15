//! pinbridge-agent: debugger agent base in Rust on the PinBridge C ABI.
//!
//! v1 skeleton: range-filtered instruction instrumentation feeding a bounded
//! event ring through the ABI v1.1 fixed capture entries. On process fini a
//! summary (per-kind counters + newest events) lands in pinbridge-agent.log.

mod arch;
mod bp;
mod child_process;
mod context;
mod control;
mod diag;
mod disasm;
mod engines;
mod event;
mod exception;
mod hooks;
mod high_priority;
mod lifecycle;
mod log;
mod modules;
mod priority;
mod query_server;
mod record;
mod resolve;
mod ring;
mod sync_intercept;
#[cfg(feature = "scripting")]
mod scripting;
#[cfg(not(feature = "scripting"))]
#[path = "scripting_stub.rs"]
mod scripting;
mod stepper;
mod syscall_engine;

use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicU64, Ordering};
use event::{EVENT_BRANCH_EDGE, EVENT_EXEC, EVENT_HOOK_REGS, EVENT_MEMORY};
use pinbridge_sys::*;

static ENTRY_BP_ADDRESS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn entry_bp_address() -> u64 {
    ENTRY_BP_ADDRESS.load(Ordering::Acquire)
}

/// TLS-free map/set aliases. std's default `RandomState` caches its hash
/// keys in a `thread_local!` (std::hash::random), and this module's TLS
/// index is never assigned — Pin maps the agent DLL privately. A
/// `HashMap::new()` on a Pin internal thread therefore reads/writes whatever
/// foreign slot the unassigned index aliases (the field-observed heap
/// corruption: those writes land in live per-thread structures), which is
/// why no code in this crate may create a RandomState-backed map on those
/// threads. The fixed-key SipHash hasher never touches TLS; the keys hashed
/// here (addresses, plugin names) are not adversarial, so deterministic
/// hashing is fine.
pub type TlsFreeBuild =
    std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>;
pub type TlsFreeMap<K, V> = std::collections::HashMap<K, V, TlsFreeBuild>;
pub type TlsFreeSet<K> = std::collections::HashSet<K, TlsFreeBuild>;

pub fn new_map<K, V>() -> TlsFreeMap<K, V> {
    TlsFreeMap::default()
}

pub fn new_set<K>() -> TlsFreeSet<K> {
    TlsFreeSet::default()
}

fn agent_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    log::init();
    unsafe {
        // Routine-name instrumentation (including the early process-exit
        // edge) requires Pin's symbol manager. Pin requires this before
        // pb_pin_init/PIN_Init.
        let symbols_status = pb_pin_init_symbols();
        log::line(&format!("pb_pin_init_symbols -> {symbols_status}"));
        if symbols_status != PB_OK {
            return 11;
        }
        if pb_pin_init(argc, argv) != PB_OK {
            log::line("pb_pin_init failed");
            return 1;
        }
        log::line("pb_pin_init ok");
        log::line(&format!(
            "arch={} pointer_width={}",
            arch::name(),
            arch::pointer_width()
        ));
        diag::install(); // Pin APIs inside: must run after pb_pin_init
        if ring::init() != PB_OK {
            log::line("ring init failed");
            return 3;
        }
        if priority::init() != PB_OK {
            log::line("priority queue init failed");
            return 12;
        }
        let bp_status = bp::init();
        log::line(&format!("breakpoint engine init -> {bp_status}"));
        if bp_status != PB_OK {
            return 5;
        }
        let hook_status = hooks::init();
        log::line(&format!("hook action engine init -> {hook_status}"));
        if hook_status != PB_OK {
            return 9;
        }
        let sync_intercept_status = sync_intercept::init();
        log::line(&format!(
            "synchronous interceptor init -> {sync_intercept_status}"
        ));
        if sync_intercept_status != PB_OK {
            return 14;
        }
        if engines::meta_init() != PB_OK {
            log::line("meta init failed");
            return 6;
        }
        if stepper::init() != PB_OK {
            log::line("stepper init failed");
            return 7;
        }
        engines::configure_from_env();
        let (trace_start, trace_end) = engines::trace_range();
        let (hook_start, hook_end) = engines::hook_range();
        log::line(&format!(
            "ranges: trace=0x{trace_start:x}-0x{trace_end:x} hook=0x{hook_start:x}-0x{hook_end:x}"
        ));
        let spawn_status = query_server::spawn();
        log::line(&format!("query server spawn -> {spawn_status}"));
        if spawn_status != PB_OK {
            return 4;
        }
        let script_status = scripting::spawn(query_server::BOUND_PORT.load(
            core::sync::atomic::Ordering::Acquire,
        ));
        log::line(&format!("scripting spawn -> {script_status}"));
        if script_status != PB_OK {
            return 8;
        }

        let mut instrument_handle = PbCallbackHandle { opaque: 0 };
        if pb_ins_add_instrument_function(
            Some(engines::on_ins),
            core::ptr::null_mut(),
            &mut instrument_handle,
        ) != PB_OK
        {
            log::line("pb_ins_add_instrument_function failed");
            return 2;
        }
        let mut fini_handle = PbCallbackHandle { opaque: 0 };
        pb_pin_add_fini_function(Some(on_fini), core::ptr::null_mut(), &mut fini_handle);
        let syscall_status = syscall_engine::register();
        let exception_status = exception::register();
        let modules_status = modules::register();
        let lifecycle_status = lifecycle::register();
        let (oom_status, detach_status) = high_priority::register();
        let child_status = child_process::init_and_register();
        log::line(&format!(
            "engines: syscall -> {syscall_status}, exception -> {exception_status}, modules -> {modules_status}, lifecycle -> {lifecycle_status}, oom -> {oom_status}, detach -> {detach_status}, child.follow -> {child_status}"
        ));
        if lifecycle_status != PB_OK {
            return 10;
        }
        if child_status != PB_OK {
            return 13;
        }
        if std::env::var("PINBRIDGE_ENTRY_BP").ok().as_deref() == Some("1") {
            // The main image is not in the image list at tool-init time, so
            // plant the one-shot entry breakpoint from the image-load
            // callback instead (still before the first instruction runs).
            let mut img_handle = PbCallbackHandle { opaque: 0 };
            if pb_img_add_instrument_function(
                Some(on_img_load),
                core::ptr::null_mut(),
                &mut img_handle,
            ) != PB_OK
            {
                log::line("entry bp: img callback registration failed");
            }
        }
        log::line("start program");

        pb_pin_start_program_default()
    }
}

/// One-shot breakpoint on the main module's entry point (PINBRIDGE_ENTRY_BP=1,
/// set by the GUI launcher). Fires per loaded image; only the main one is used.
unsafe extern "C" fn on_img_load(img: PbImgHandle, _user_data: *mut c_void) {
    let mut is_main: u8 = 0;
    pb_img_is_main_executable(img, &mut is_main);
    if is_main == 0 {
        return;
    }
    let mut entry: u64 = 0;
    if pb_img_entry_address(img, &mut entry) != PB_OK || entry == 0 {
        log::line("entry bp: no entry address");
        return;
    }
    match bp::set_oneshot(entry) {
        Ok(id) => {
            ENTRY_BP_ADDRESS.store(entry, Ordering::Release);
            log::line(&format!("entry bp #{id} at 0x{entry:x}"));
        }
        Err(status) => log::line(&format!("entry bp failed -> {status}")),
    }
}

// Rust's lib-test harness supplies its own `main`.  Export the Pin tool entry
// only for the real cdylib; otherwise `cargo test -p pinbridge-agent --lib`
// links two main symbols and none of the agent's pure unit tests can run.
#[cfg(not(test))]
pinbridge_tool::tool_entry!(agent_main);

unsafe extern "C" fn on_fini(code: i32, _user_data: *mut c_void) {
    lifecycle::record_fini(code);
    let (exit_probes, exit_hits) = lifecycle::exit_probe_counts();
    let (child_decisions, child_follow, child_reject) = child_process::decision_counts();
    let (sync_decisions, sync_timeouts, sync_busy) = sync_intercept::stats();
    crate::log::line(&format!(
        "fini code={code} exit_probes={exit_probes} exit_hits={exit_hits} priority_total={} priority_dropped={} child_decisions={child_decisions} child_follow={child_follow} child_reject={child_reject} child_decision_timeouts={} sync_decisions={sync_decisions} sync_timeouts={sync_timeouts} sync_busy={sync_busy}",
        priority::total(),
        priority::dropped(),
        child_process::timeout_count(),
    ));
    let (trace_start, trace_end) = engines::trace_range();
    let (hook_start, hook_end) = engines::hook_range();
    // Copy under a try-locked Pin mutex into a reserved buffer; formatting
    // (allocation) stays OUTSIDE the critical section — allocating under a
    // Pin mutex deadlocks against heap-holding threads (see ring.rs).
    let mut newest = Vec::with_capacity(16);
    let _ = ring::try_drain_newest(16, &mut newest); // busy: summary without the tail
    let mut report = String::new();
    report.push_str("pinbridge-agent summary\n");
    report.push_str(&format!(
        "abi={}.{} trace_range=0x{trace_start:x}-0x{trace_end:x} hook_range=0x{hook_start:x}-0x{hook_end:x}\n",
        PB_ABI_VERSION_MAJOR, PB_ABI_VERSION_MINOR
    ));
    let total = ring::ring_total();
    let retained = total.min(ring::RING_CAPACITY as u64);
    report.push_str(&format!(
        "total={} dropped={} | hook_regs={} memory={} exec={} branch_edge={}\n",
        total,
        ring::total_seq().saturating_sub(retained),
        ring::kind_count(EVENT_HOOK_REGS as usize),
        ring::kind_count(EVENT_MEMORY as usize),
        ring::kind_count(EVENT_EXEC as usize),
        ring::kind_count(EVENT_BRANCH_EDGE as usize),
    ));
    report.push_str("newest events:\n");
    for event in newest {
        report.push_str(&format!(
            "  #{:<6} {:<11} tid={:<4} ip=0x{:x} arg0=0x{:x} arg1=0x{:x} arg2=0x{:x}\n",
            event.sequence,
            event.kind_name(),
            event.thread_id,
            event.address,
            event.arg0,
            event.arg1,
            event.arg2
        ));
    }
    crate::log::append_block(&report);
}
