//! Named Python event subscriptions and their stable event dictionaries.
//!
//! This is deliberately separate from host.rs: the host owns scheduling,
//! while this module owns public event names, matching and Python schemas.

use crate::event::*;
use core::sync::atomic::{AtomicU64, Ordering};
use pinbridge_proto::EventRecord;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::cell::RefCell;
use std::rc::Rc;

const CONTEXT_CHANGE_EXCEPTION: u64 = 4;
static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

const GENERATION_WINDOW_BITS: u64 = 65_536;
const GENERATION_WINDOW_WORDS: usize = GENERATION_WINDOW_BITS as usize / 64;

/// Bounded exact-once window for mirrored events whose native producers may
/// publish out of generation order on different application threads.
pub struct GenerationWindow {
    ignore_through: u64,
    highest: u64,
    bits: Box<[u64]>,
}

impl GenerationWindow {
    pub fn new(ignore_through: u64) -> Self {
        Self {
            ignore_through,
            highest: ignore_through,
            bits: vec![0; GENERATION_WINDOW_WORDS].into_boxed_slice(),
        }
    }

    /// True only for the first copy of a generation inside the retained
    /// window. Generations older than a full main-ring-sized window are
    /// treated as stale rather than risking a duplicate Python callback.
    pub fn accept(&mut self, generation: u64) -> bool {
        if generation == 0 || generation <= self.ignore_through {
            return false;
        }
        if generation > self.highest {
            let advance = generation - self.highest;
            if advance >= GENERATION_WINDOW_BITS {
                self.bits.fill(0);
            } else {
                for value in self.highest + 1..=generation {
                    let bit = value % GENERATION_WINDOW_BITS;
                    self.bits[bit as usize / 64] &= !(1u64 << (bit % 64));
                }
            }
            self.highest = generation;
        } else if self.highest - generation >= GENERATION_WINDOW_BITS {
            return false;
        }
        let bit = generation % GENERATION_WINDOW_BITS;
        let word = &mut self.bits[bit as usize / 64];
        let mask = 1u64 << (bit % 64);
        if *word & mask != 0 {
            return false;
        }
        *word |= mask;
        true
    }
}

pub type SharedGenerationWindow = Rc<RefCell<GenerationWindow>>;

pub fn shared_generation_window(ignore_through: u64) -> SharedGenerationWindow {
    Rc::new(RefCell::new(GenerationWindow::new(ignore_through)))
}

pub fn context_reason_name(reason: u64) -> &'static str {
    match reason {
        0 => "fatal_signal",
        1 => "signal",
        2 => "signal_return",
        3 => "apc",
        4 => "exception",
        5 => "callback",
        _ => "unknown",
    }
}

pub const PUBLIC_EVENT_NAMES: [&str; 28] = [
    "process.start",
    "process.exit",
    "process.prepare_fini",
    "thread.start",
    "thread.exit",
    "module.load",
    "module.unload",
    "exception",
    "context.change",
    "syscall",
    "hook.entry",
    "hook.return",
    "instruction",
    "instruction.decode",
    "memory",
    "branch.edge",
    "code.smc",
    "pin.detach",
    "pin.attach",
    "memory.oom",
    "pin.internal_exception",
    "debugger.breakpoint",
    "debugger.single_step",
    "debugger.async_break",
    "trace.instrument",
    "routine.instrument",
    "basic_block.instrument",
    "execution.trap",
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EventSelector {
    Kind(u32),
    Exception,
    ProcessExit,
    ProcessPrepareFini,
}

impl EventSelector {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "hook" | "hook.entry" | "hook_entry" | "hook_regs" => Self::Kind(EVENT_HOOK_REGS),
            "hook.return" | "hook_return" => Self::Kind(EVENT_HOOK_RETURN),
            "memory" | "mem" => Self::Kind(EVENT_MEMORY),
            "instruction" | "instruction.exec" | "exec" => Self::Kind(EVENT_EXEC),
            "instruction.decode" | "instruction_decode" | "decode" => {
                Self::Kind(EVENT_INSTRUCTION_DECODE)
            }
            "branch" | "branch.edge" | "branch_edge" => Self::Kind(EVENT_BRANCH_EDGE),
            "syscall" => Self::Kind(EVENT_SYSCALL),
            "context" | "context.change" | "context_change" => Self::Kind(EVENT_CONTEXT_CHANGE),
            "exception" => Self::Exception,
            "module.load" | "module_load" => Self::Kind(EVENT_MODULE_LOAD),
            "module.unload" | "module_unload" => Self::Kind(EVENT_MODULE_UNLOAD),
            "thread.start" | "thread_start" => Self::Kind(EVENT_THREAD_START),
            "thread.exit" | "thread_exit" | "thread.fini" | "thread_fini" => {
                Self::Kind(EVENT_THREAD_EXIT)
            }
            "process.start" | "process_start" | "application.start" => {
                Self::Kind(EVENT_PROCESS_START)
            }
            "process.exit" | "process_exit" => Self::ProcessExit,
            "process.prepare_fini"
            | "process_prepare_fini"
            | "process.exit.prepare"
            | "prepare_fini" => Self::ProcessPrepareFini,
            "code.smc" | "smc" | "self_modifying_code" => Self::Kind(EVENT_SMC),
            "pin.detach" | "pin_detach" => Self::Kind(EVENT_PIN_DETACH),
            "pin.attach" | "pin_attach" => Self::Kind(EVENT_PIN_ATTACH),
            "memory.oom" | "out_of_memory" | "oom" => Self::Kind(EVENT_OUT_OF_MEMORY),
            "pin.internal_exception" | "pin_internal_exception" | "internal_exception" => {
                Self::Kind(EVENT_PIN_INTERNAL_EXCEPTION)
            }
            "debugger.breakpoint" | "debugger_breakpoint" => Self::Kind(EVENT_DEBUGGER_BREAKPOINT),
            "debugger.single_step" | "debugger_single_step" => {
                Self::Kind(EVENT_DEBUGGER_SINGLE_STEP)
            }
            "debugger.async_break" | "debugger_async_break" => {
                Self::Kind(EVENT_DEBUGGER_ASYNC_BREAK)
            }
            "trace.instrument" | "trace_instrument" => Self::Kind(EVENT_TRACE_INSTRUMENT),
            "routine.instrument" | "routine_instrument" | "function.instrument" => {
                Self::Kind(EVENT_ROUTINE_INSTRUMENT)
            }
            "basic_block.instrument"
            | "basic_block_instrument"
            | "bbl.instrument"
            | "bbl_instrument" => Self::Kind(EVENT_BBL_INSTRUMENT),
            "execution.trap" | "execution_trap" | "exec.trap" | "exec_trap" => {
                Self::Kind(EVENT_EXECUTION_TRAP)
            }
            _ => return None,
        })
    }

    pub fn event_type(self) -> &'static str {
        match self {
            Self::Exception => "exception",
            Self::ProcessExit => "process.exit",
            Self::ProcessPrepareFini => "process.prepare_fini",
            Self::Kind(EVENT_HOOK_REGS) => "hook.entry",
            Self::Kind(EVENT_HOOK_RETURN) => "hook.return",
            Self::Kind(EVENT_MEMORY) => "memory",
            Self::Kind(EVENT_EXEC) => "instruction",
            Self::Kind(EVENT_INSTRUCTION_DECODE) => "instruction.decode",
            Self::Kind(EVENT_BRANCH_EDGE) => "branch.edge",
            Self::Kind(EVENT_SYSCALL) => "syscall",
            Self::Kind(EVENT_CONTEXT_CHANGE) => "context.change",
            Self::Kind(EVENT_MODULE_LOAD) => "module.load",
            Self::Kind(EVENT_MODULE_UNLOAD) => "module.unload",
            Self::Kind(EVENT_THREAD_START) => "thread.start",
            Self::Kind(EVENT_THREAD_EXIT) => "thread.exit",
            Self::Kind(EVENT_PROCESS_START) => "process.start",
            Self::Kind(EVENT_PROCESS_EXIT) => "process.exit",
            Self::Kind(EVENT_SMC) => "code.smc",
            Self::Kind(EVENT_PIN_DETACH) => "pin.detach",
            Self::Kind(EVENT_PIN_ATTACH) => "pin.attach",
            Self::Kind(EVENT_OUT_OF_MEMORY) => "memory.oom",
            Self::Kind(EVENT_PIN_INTERNAL_EXCEPTION) => "pin.internal_exception",
            Self::Kind(EVENT_DEBUGGER_BREAKPOINT) => "debugger.breakpoint",
            Self::Kind(EVENT_DEBUGGER_SINGLE_STEP) => "debugger.single_step",
            Self::Kind(EVENT_DEBUGGER_ASYNC_BREAK) => "debugger.async_break",
            Self::Kind(EVENT_TRACE_INSTRUMENT) => "trace.instrument",
            Self::Kind(EVENT_ROUTINE_INSTRUMENT) => "routine.instrument",
            Self::Kind(EVENT_BBL_INSTRUMENT) => "basic_block.instrument",
            Self::Kind(EVENT_EXECUTION_TRAP) => "execution.trap",
            Self::Kind(_) => "unknown",
        }
    }

    pub fn matches(self, event: &EventRecord) -> bool {
        match self {
            Self::ProcessExit => {
                event.kind == EVENT_PROCESS_EXIT
                    && (event.arg1 == PROCESS_EXIT_SOURCE_API
                        || (event.arg1 == PROCESS_EXIT_SOURCE_PREPARE_FINI && event.arg2 == 0))
            }
            Self::ProcessPrepareFini => {
                event.kind == EVENT_PROCESS_EXIT && event.arg1 == PROCESS_EXIT_SOURCE_PREPARE_FINI
            }
            Self::Kind(kind) => event.kind == kind,
            Self::Exception => {
                event.kind == EVENT_CONTEXT_CHANGE && event.arg0 == CONTEXT_CHANGE_EXCEPTION
            }
        }
    }

    pub fn is_sticky(self) -> bool {
        matches!(
            self,
            Self::Kind(EVENT_PROCESS_START) | Self::ProcessExit | Self::ProcessPrepareFini
        )
    }

    pub fn is_priority(self) -> bool {
        matches!(
            self,
            Self::Exception
                | Self::ProcessExit
                | Self::ProcessPrepareFini
                | Self::Kind(EVENT_CONTEXT_CHANGE)
                | Self::Kind(EVENT_THREAD_START)
                | Self::Kind(EVENT_THREAD_EXIT)
                | Self::Kind(EVENT_PROCESS_START)
                | Self::Kind(EVENT_MODULE_LOAD)
                | Self::Kind(EVENT_MODULE_UNLOAD)
                | Self::Kind(EVENT_SMC)
                | Self::Kind(EVENT_PIN_DETACH)
                | Self::Kind(EVENT_PIN_ATTACH)
                | Self::Kind(EVENT_OUT_OF_MEMORY)
                | Self::Kind(EVENT_PIN_INTERNAL_EXCEPTION)
                | Self::Kind(EVENT_DEBUGGER_BREAKPOINT)
                | Self::Kind(EVENT_DEBUGGER_SINGLE_STEP)
                | Self::Kind(EVENT_DEBUGGER_ASYNC_BREAK)
                | Self::Kind(EVENT_EXECUTION_TRAP)
        )
    }

    /// Module events are intentionally mirrored: Python prefers the
    /// high-priority copy but must keep a compatibility-ring cursor so that
    /// either try-lock path can recover the other.
    pub fn uses_compatibility_ring(self) -> bool {
        (!self.is_priority()
            && !matches!(
                self,
                Self::Kind(EVENT_HOOK_REGS) | Self::Kind(EVENT_HOOK_RETURN)
            ))
            || matches!(
                self,
                Self::Exception
                    | Self::Kind(EVENT_CONTEXT_CHANGE)
                    | Self::Kind(EVENT_MODULE_LOAD)
                    | Self::Kind(EVENT_MODULE_UNLOAD)
            )
    }

    pub fn requires_smc_registration(self) -> bool {
        self == Self::Kind(EVENT_SMC)
    }
}

pub struct EventSubscription {
    pub selector: EventSelector,
    pub callback: Py<PyAny>,
    pub once: bool,
    pub order: u64,
    /// Per-lane native edges captured at the exact registration point. The
    /// plugin cursor starts earlier so events arriving during pb_init remain
    /// readable, while these boundaries suppress records predating this
    /// specific handler.
    pub main_start_after: u64,
    pub priority_start_after: u64,
    pub observation_start_after: u64,
    /// Optional exact address for a named Hook observer. Supplying one also
    /// acquires ownership of the corresponding native Hook point.
    pub hook_address: Option<u64>,
    /// Per-handler native syscall-number interest. None means all numbers.
    /// Other selector kinds always keep this as None.
    pub syscall_numbers: Option<crate::TlsFreeSet<u32>>,
    /// Exact-once state shared with the current dispatch snapshot. This is
    /// separate per handler so named and legacy APIs may coexist.
    pub syscall_generations: Option<SharedGenerationWindow>,
    /// Exact-once state for all mirrored context-change reasons. A bitmap is
    /// required because different application threads may publish native
    /// generations out of order.
    pub context_generations: Option<SharedGenerationWindow>,
    /// Sticky process events are replayed once to handlers registered after
    /// the native edge.  This flag is per subscription, so adding a second
    /// handler later still receives the current lifecycle state.
    pub sticky_delivered: bool,
    /// Latest allocation-failure occurrence delivered from either the
    /// priority ring or the emergency latest-value slot.
    pub oom_generation: u64,
    /// Latest mirrored module edge delivered from either the high-priority
    /// or compatibility ring. Context changes use the out-of-order window.
    pub mirror_generation: u64,
}

impl EventSubscription {
    pub fn new(
        selector: EventSelector,
        callback: Py<PyAny>,
        once: bool,
        hook_address: Option<u64>,
        syscall_numbers: Option<crate::TlsFreeSet<u32>>,
    ) -> (u64, Self) {
        let id = NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed);
        let oom_generation = if selector == EventSelector::Kind(EVENT_OUT_OF_MEMORY) {
            crate::high_priority::oom_snapshot()
                .map(|snapshot| snapshot.0)
                .unwrap_or(0)
        } else {
            0
        };
        let mirror_generation = if matches!(
            selector,
            EventSelector::Kind(EVENT_MODULE_LOAD) | EventSelector::Kind(EVENT_MODULE_UNLOAD)
        ) {
            crate::modules::generation()
        } else {
            0
        };
        (
            id,
            Self {
                selector,
                callback,
                once,
                order: id,
                main_start_after: crate::ring::ring_total(),
                priority_start_after: crate::priority::total(),
                observation_start_after: crate::observation::total(),
                hook_address,
                syscall_numbers,
                syscall_generations: (selector == EventSelector::Kind(EVENT_SYSCALL))
                    .then(|| shared_generation_window(crate::syscall_engine::generation())),
                context_generations: matches!(
                    selector,
                    EventSelector::Exception | EventSelector::Kind(EVENT_CONTEXT_CHANGE)
                )
                .then(|| shared_generation_window(crate::exception::generation())),
                sticky_delivered: false,
                oom_generation,
                mirror_generation,
            },
        )
    }
}

pub fn synthetic_process_event(kind: u32) -> EventRecord {
    EventRecord {
        kind,
        thread_id: pinbridge_sys::PB_INVALID_THREAD_ID,
        ..EventRecord::default()
    }
}

/// Returns the occurrence carried by a new OOM record. The emergency slot
/// and priority ring can both expose the same native callback, so consumers
/// use this generation check to invoke Python exactly once.
pub fn unseen_oom_occurrence(last_delivered: u64, event: &EventRecord) -> Option<u64> {
    (event.kind == EVENT_OUT_OF_MEMORY && event.arg1 != 0 && event.arg1 > last_delivered)
        .then_some(event.arg1)
}

/// Returns the generation carried by a new module edge. Module callbacks are
/// mirrored into two rings for compatibility and reliability, so Python must
/// consume both copies exactly once per handler.
pub fn unseen_module_generation(last_delivered: u64, event: &EventRecord) -> Option<u64> {
    (matches!(event.kind, EVENT_MODULE_LOAD | EVENT_MODULE_UNLOAD)
        && event.arg3 != 0
        && event.arg3 > last_delivered)
        .then_some(event.arg3)
}

/// Builds the public event object.  Stable descriptive fields coexist with
/// raw a0..a7 fields so new native payloads remain inspectable before a new
/// convenience schema is added.
pub fn build_event_dict(
    py: Python<'_>,
    selector: EventSelector,
    event: &EventRecord,
    module_name: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let row = PyDict::new_bound(py);
    row.set_item("type", selector.event_type())?;
    row.set_item("sequence", event.sequence)?;
    row.set_item("seq", event.sequence)?;
    row.set_item("kind", event.kind)?;
    row.set_item("kind_name", crate::event::kind_name(event.kind))?;
    let tid: i64 = if event.thread_id == pinbridge_sys::PB_INVALID_THREAD_ID {
        -1
    } else {
        event.thread_id as i64
    };
    row.set_item("thread_id", tid)?;
    row.set_item("tid", tid)?;
    row.set_item("address", event.address)?;
    row.set_item("addr", event.address)?;
    row.set_item("a0", event.arg0)?;
    row.set_item("a1", event.arg1)?;
    row.set_item("a2", event.arg2)?;
    row.set_item("a3", event.arg3)?;
    row.set_item("a4", event.arg4)?;
    row.set_item("a5", event.arg5)?;
    row.set_item("a6", event.arg6)?;
    row.set_item("a7", event.arg7)?;

    match selector {
        EventSelector::Kind(EVENT_HOOK_REGS) => {
            let registers = PyDict::new_bound(py);
            for (index, register) in crate::arch::hook_arg_regs().iter().enumerate() {
                if let Some(name) = crate::arch::gp_name(*register) {
                    registers.set_item(
                        name,
                        [event.arg0, event.arg1, event.arg2, event.arg3][index],
                    )?;
                }
            }
            row.set_item("registers", registers)?;
            row.set_item(
                "stack_arguments",
                [event.arg4, event.arg5, event.arg6, event.arg7],
            )?;
        }
        EventSelector::Kind(EVENT_HOOK_RETURN) => {
            let registers = PyDict::new_bound(py);
            for (index, register) in crate::arch::hook_arg_regs().iter().enumerate() {
                if let Some(name) = crate::arch::gp_name(*register) {
                    registers.set_item(
                        name,
                        [event.arg1, event.arg2, event.arg3, event.arg4][index],
                    )?;
                }
            }
            row.set_item("return_value", event.arg0)?;
            row.set_item("registers", registers)?;
            row.set_item("stack_arguments", [event.arg5, event.arg6, event.arg7])?;
        }
        EventSelector::Exception => {
            row.set_item("reason", event.arg0)?;
            row.set_item("reason_name", context_reason_name(event.arg0))?;
            row.set_item("code", event.arg1)?;
            row.set_item("ip", event.arg2)?;
            row.set_item("exception_generation", event.arg3)?;
            row.set_item("context_generation", event.arg3)?;
        }
        EventSelector::Kind(EVENT_CONTEXT_CHANGE) => {
            row.set_item("reason", event.arg0)?;
            row.set_item("reason_name", context_reason_name(event.arg0))?;
            row.set_item("info", event.arg1 as i64)?;
            row.set_item("ip", event.arg2)?;
            row.set_item("from_ip", event.arg2)?;
            row.set_item("from_ip_known", event.arg6 != 0)?;
            row.set_item("to_ip", event.arg4)?;
            row.set_item("to_ip_known", event.arg5 != 0)?;
            row.set_item("context_generation", event.arg3)?;
            row.set_item(
                "exception_generation",
                if event.arg0 == CONTEXT_CHANGE_EXCEPTION {
                    event.arg3
                } else {
                    0
                },
            )?;
        }
        EventSelector::Kind(EVENT_THREAD_START) => {
            row.set_item("ip", event.address)?;
            row.set_item("flags", event.arg0 as i64)?;
        }
        EventSelector::Kind(EVENT_THREAD_EXIT) => {
            row.set_item("ip", event.address)?;
            row.set_item("exit_code", event.arg0 as i64)?;
        }
        EventSelector::Kind(EVENT_PROCESS_START) => {
            row.set_item("phase", "start")?;
        }
        EventSelector::ProcessExit => {
            row.set_item("phase", "exiting")?;
            row.set_item("exit_code", event.arg0 as i64)?;
            row.set_item("exit_code_known", event.arg1 == PROCESS_EXIT_SOURCE_API)?;
            row.set_item(
                "source",
                if event.arg1 == PROCESS_EXIT_SOURCE_API {
                    "exit_api"
                } else {
                    "prepare_fini"
                },
            )?;
        }
        EventSelector::ProcessPrepareFini => {
            row.set_item("phase", "prepare_fini")?;
            row.set_item("exit_code", event.arg0 as i64)?;
            row.set_item("exit_code_known", event.arg2 != 0)?;
            row.set_item("had_exit_request", event.arg2 != 0)?;
            row.set_item("native_prepare_reached", event.arg3 != 0)?;
            row.set_item(
                "trigger",
                if event.arg3 != 0 {
                    "pin_prepare_for_fini"
                } else {
                    "exit_api"
                },
            )?;
            row.set_item("source", "prepare_fini")?;
        }
        EventSelector::Kind(EVENT_SMC) => {
            row.set_item("trace_start", event.arg0)?;
            row.set_item("trace_end", event.arg1)?;
        }
        EventSelector::Kind(EVENT_PIN_DETACH) => {
            row.set_item("phase", "detached")?;
        }
        EventSelector::Kind(EVENT_PIN_ATTACH) => {
            row.set_item("phase", "attached")?;
        }
        EventSelector::Kind(EVENT_OUT_OF_MEMORY) => {
            row.set_item("requested_size", event.arg0)?;
            row.set_item("occurrence", event.arg1)?;
            row.set_item("recovered_from_emergency_slot", event.arg2 != 0)?;
        }
        EventSelector::Kind(EVENT_PIN_INTERNAL_EXCEPTION) => {
            row.set_item("ip", event.address)?;
            row.set_item("code", event.arg0)?;
            row.set_item("exception_address", event.arg1)?;
            row.set_item("fault_address", event.arg2)?;
            row.set_item("fault_address_known", event.arg5 != 0)?;
            row.set_item("access_type", event.arg3)?;
            row.set_item("exception_class", event.arg4)?;
        }
        EventSelector::Kind(EVENT_DEBUGGER_BREAKPOINT)
        | EventSelector::Kind(EVENT_DEBUGGER_SINGLE_STEP)
        | EventSelector::Kind(EVENT_DEBUGGER_ASYNC_BREAK) => {
            row.set_item("ip", event.address)?;
            row.set_item("debugging_event", event.arg0)?;
            row.set_item("stack_pointer", event.arg1)?;
            row.set_item("flags", event.arg2)?;
            row.set_item("return_value", event.arg3)?;
        }
        EventSelector::Kind(EVENT_TRACE_INSTRUMENT) => {
            row.set_item("size", event.arg0)?;
            row.set_item("basic_block_count", event.arg1)?;
            row.set_item("instruction_count", event.arg2)?;
            row.set_item("has_fall_through", event.arg3 != 0)?;
            row.set_item("routine_address", event.arg4)?;
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_ROUTINE_INSTRUMENT) => {
            row.set_item("size", event.arg0)?;
            row.set_item("instruction_count", event.arg1)?;
            row.set_item("routine_id", event.arg2)?;
            row.set_item("is_dynamic", event.arg3 != 0)?;
            row.set_item("is_artificial", event.arg4 != 0)?;
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_BBL_INSTRUMENT) => {
            row.set_item("size", event.arg0)?;
            row.set_item("instruction_count", event.arg1)?;
            row.set_item("has_fall_through", event.arg2 != 0)?;
            row.set_item("is_original", event.arg3 != 0)?;
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_EXECUTION_TRAP) => {
            row.set_item("id", event.arg0)?;
            row.set_item("start", event.arg1)?;
            row.set_item("end", event.arg2)?;
            row.set_item("hits", event.arg3)?;
            row.set_item("stop_generation", event.arg4)?;
            row.set_item("once", event.arg5 & 1 != 0)?;
            if event.arg5 & 2 != 0 {
                row.set_item("thread_filter", event.arg6)?;
            } else {
                row.set_item("thread_filter", py.None())?;
            }
        }
        EventSelector::Kind(EVENT_MODULE_LOAD) => {
            row.set_item("base", event.arg0)?;
            row.set_item("end", event.arg1)?;
            row.set_item("is_main", event.arg2 != 0)?;
            row.set_item("module_generation", event.arg3)?;
            if let Some(name) = module_name {
                row.set_item("name", name)?;
            }
        }
        EventSelector::Kind(EVENT_MODULE_UNLOAD) => {
            row.set_item("base", event.arg0)?;
            row.set_item("module_generation", event.arg3)?;
            if let Some(name) = module_name {
                row.set_item("name", name)?;
            }
        }
        EventSelector::Kind(EVENT_SYSCALL) => {
            let phase = if event.arg1 == 0 { "enter" } else { "exit" };
            row.set_item("number", event.arg0)?;
            row.set_item("phase", phase)?;
            row.set_item("syscall_generation", event.address)?;
            if event.arg1 == 0 {
                row.set_item(
                    "args",
                    [
                        event.arg2, event.arg3, event.arg4, event.arg5, event.arg6, event.arg7,
                    ],
                )?;
            } else {
                row.set_item("retval", event.arg3)?;
                row.set_item("errno", event.arg4)?;
            }
        }
        EventSelector::Kind(EVENT_MEMORY) => {
            row.set_item("memory_address", event.arg0)?;
            row.set_item("size", event.arg1)?;
            row.set_item("access", event.arg2)?;
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_EXEC) => {
            row.set_item("size", event.arg0)?;
            let next_address = event.address.wrapping_add(event.arg0);
            row.set_item(
                "next_address",
                if crate::arch::is_32() {
                    next_address & u32::MAX as u64
                } else {
                    next_address
                },
            )?;
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_INSTRUCTION_DECODE) => {
            row.set_item("size", event.arg0)?;
            let next_address = event.address.wrapping_add(event.arg0);
            row.set_item(
                "next_address",
                if crate::arch::is_32() {
                    next_address & u32::MAX as u64
                } else {
                    next_address
                },
            )?;
            row.set_item("category", event.arg1)?;
            row.set_item("extension", event.arg2)?;
            row.set_item("opcode", event.arg3)?;
            row.set_item("memory_operand_count", event.arg4)?;
            row.set_item(
                "has_fall_through",
                event.arg5 & DECODE_FLAG_FALL_THROUGH != 0,
            )?;
            row.set_item("is_branch", event.arg5 & DECODE_FLAG_BRANCH != 0)?;
            row.set_item("is_call", event.arg5 & DECODE_FLAG_CALL != 0)?;
            row.set_item("is_return", event.arg5 & DECODE_FLAG_RETURN != 0)?;
            row.set_item("is_syscall", event.arg5 & DECODE_FLAG_SYSCALL != 0)?;
            let direct = event.arg5 & DECODE_FLAG_DIRECT_CONTROL_FLOW != 0;
            row.set_item("is_direct_control_flow", direct)?;
            row.set_item(
                "is_indirect_control_flow",
                event.arg5 & DECODE_FLAG_INDIRECT_CONTROL_FLOW != 0,
            )?;
            if direct {
                row.set_item("direct_target", event.arg6)?;
            } else {
                row.set_item("direct_target", py.None())?;
            }
            row.set_item("policy_generation", event.arg7)?;
        }
        EventSelector::Kind(EVENT_BRANCH_EDGE) => {
            row.set_item("target", event.arg0)?;
            row.set_item("taken", event.arg1 != 0)?;
            row.set_item("policy_generation", event.arg7)?;
        }
        _ => {}
    }
    Ok(row.into_any().unbind())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_names_and_aliases_are_stable() {
        assert_eq!(
            EventSelector::parse("thread.start"),
            Some(EventSelector::Kind(EVENT_THREAD_START))
        );
        assert_eq!(
            EventSelector::parse("thread_fini"),
            Some(EventSelector::Kind(EVENT_THREAD_EXIT))
        );
        assert_eq!(
            EventSelector::parse("exception"),
            Some(EventSelector::Exception)
        );
        assert_eq!(EventSelector::parse("not-an-event"), None);
        assert_eq!(
            EventSelector::parse("process.exit"),
            Some(EventSelector::ProcessExit)
        );
        assert_eq!(
            EventSelector::parse("process.prepare_fini"),
            Some(EventSelector::ProcessPrepareFini)
        );
        for name in [
            "module.load",
            "module.unload",
            "code.smc",
            "pin.detach",
            "pin.attach",
            "memory.oom",
            "pin.internal_exception",
            "debugger.breakpoint",
            "debugger.single_step",
            "debugger.async_break",
            "execution.trap",
        ] {
            let selector = EventSelector::parse(name).expect("public event must parse");
            assert!(selector.is_priority());
            assert!(PUBLIC_EVENT_NAMES.contains(&name));
        }
        assert!(EventSelector::ProcessExit.is_priority());
        assert!(EventSelector::ProcessPrepareFini.is_priority());
        assert!(EventSelector::ProcessExit.is_sticky());
        assert!(EventSelector::ProcessPrepareFini.is_sticky());
        for name in ["exception", "context.change"] {
            let selector = EventSelector::parse(name).expect("context selector");
            assert!(selector.is_priority());
            assert!(selector.uses_compatibility_ring());
        }
        for name in ["module.load", "module.unload"] {
            let selector = EventSelector::parse(name).expect("module selector");
            assert!(selector.is_priority());
            assert!(selector.uses_compatibility_ring());
        }
        for name in ["hook.entry", "hook.return"] {
            let selector = EventSelector::parse(name).expect("Hook selector");
            assert!(!selector.uses_compatibility_ring());
        }
        assert!(EventSelector::parse("code.smc")
            .expect("SMC selector")
            .requires_smc_registration());
        assert!(!EventSelector::parse("thread.start")
            .expect("thread selector")
            .requires_smc_registration());
        for name in [
            "trace.instrument",
            "routine.instrument",
            "basic_block.instrument",
        ] {
            assert!(PUBLIC_EVENT_NAMES.contains(&name));
            assert!(EventSelector::parse(name).is_some());
        }
    }

    #[test]
    fn exception_is_a_filtered_context_change() {
        let mut event = EventRecord {
            kind: EVENT_CONTEXT_CHANGE,
            arg0: CONTEXT_CHANGE_EXCEPTION,
            ..EventRecord::default()
        };
        assert!(EventSelector::Exception.matches(&event));
        event.arg0 = 1;
        assert!(!EventSelector::Exception.matches(&event));
        assert!(EventSelector::Kind(EVENT_CONTEXT_CHANGE).matches(&event));
    }

    #[test]
    fn context_reason_names_cover_the_public_pin_values() {
        assert_eq!(context_reason_name(0), "fatal_signal");
        assert_eq!(context_reason_name(1), "signal");
        assert_eq!(context_reason_name(2), "signal_return");
        assert_eq!(context_reason_name(3), "apc");
        assert_eq!(context_reason_name(4), "exception");
        assert_eq!(context_reason_name(5), "callback");
        assert_eq!(context_reason_name(6), "unknown");
    }

    #[test]
    fn process_exit_and_prepare_fini_are_distinct_selectors() {
        let exit_request = EventRecord {
            kind: EVENT_PROCESS_EXIT,
            arg1: PROCESS_EXIT_SOURCE_API,
            ..EventRecord::default()
        };
        assert!(EventSelector::ProcessExit.matches(&exit_request));
        assert!(!EventSelector::ProcessPrepareFini.matches(&exit_request));

        let prepare_after_request = EventRecord {
            kind: EVENT_PROCESS_EXIT,
            arg1: PROCESS_EXIT_SOURCE_PREPARE_FINI,
            arg2: 1,
            ..EventRecord::default()
        };
        assert!(!EventSelector::ProcessExit.matches(&prepare_after_request));
        assert!(EventSelector::ProcessPrepareFini.matches(&prepare_after_request));

        let prepare_fallback = EventRecord {
            kind: EVENT_PROCESS_EXIT,
            arg1: PROCESS_EXIT_SOURCE_PREPARE_FINI,
            arg2: 0,
            ..EventRecord::default()
        };
        assert!(EventSelector::ProcessExit.matches(&prepare_fallback));
        assert!(EventSelector::ProcessPrepareFini.matches(&prepare_fallback));
    }

    #[test]
    fn oom_occurrence_deduplicates_emergency_and_ring_records() {
        let first = EventRecord {
            kind: EVENT_OUT_OF_MEMORY,
            arg0: 0x4000,
            arg1: 7,
            arg2: 1,
            ..EventRecord::default()
        };
        assert_eq!(unseen_oom_occurrence(6, &first), Some(7));
        assert_eq!(unseen_oom_occurrence(7, &first), None);

        let newer_ring_record = EventRecord {
            arg1: 8,
            arg2: 0,
            ..first
        };
        assert_eq!(unseen_oom_occurrence(7, &newer_ring_record), Some(8));
    }

    #[test]
    fn module_generation_deduplicates_priority_and_compatibility_records() {
        let priority_copy = EventRecord {
            kind: EVENT_MODULE_LOAD,
            arg0: 0x1800_0000,
            arg1: 0x1800_ffff,
            arg3: 11,
            ..EventRecord::default()
        };
        assert_eq!(unseen_module_generation(10, &priority_copy), Some(11));

        let ring_copy = EventRecord {
            sequence: 500,
            ..priority_copy
        };
        assert_eq!(unseen_module_generation(11, &ring_copy), None);

        let unload = EventRecord {
            kind: EVENT_MODULE_UNLOAD,
            arg1: 0,
            arg3: 12,
            ..ring_copy
        };
        assert_eq!(unseen_module_generation(11, &unload), Some(12));
    }

    #[test]
    fn generation_window_deduplicates_mirrors_without_dropping_reordered_events() {
        let mut window = GenerationWindow::new(30);
        assert!(window.accept(32));
        assert!(window.accept(31));
        assert!(!window.accept(32));
        assert!(!window.accept(31));
        assert!(!window.accept(30));

        assert!(window.accept(70_000));
        assert!(!window.accept(1));
        assert!(!window.accept(1_000));
        assert!(!window.accept(70_000));
    }
}
