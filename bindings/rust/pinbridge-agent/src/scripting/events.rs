//! Named Python event subscriptions and their stable event dictionaries.
//!
//! This is deliberately separate from host.rs: the host owns scheduling,
//! while this module owns public event names, matching and Python schemas.

use crate::event::*;
use core::sync::atomic::{AtomicU64, Ordering};
use pinbridge_proto::EventRecord;
use pyo3::prelude::*;
use pyo3::types::PyDict;

const CONTEXT_CHANGE_EXCEPTION: u64 = 4;
static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

pub const PUBLIC_EVENT_NAMES: [&str; 20] = [
    "process.start",
    "process.exit",
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
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EventSelector {
    Kind(u32),
    Exception,
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
            "process.exit" | "process_exit" | "process.exit.prepare" | "prepare_fini" => {
                Self::Kind(EVENT_PROCESS_EXIT)
            }
            "code.smc" | "smc" | "self_modifying_code" => Self::Kind(EVENT_SMC),
            "pin.detach" | "pin_detach" => Self::Kind(EVENT_PIN_DETACH),
            "pin.attach" | "pin_attach" => Self::Kind(EVENT_PIN_ATTACH),
            "memory.oom" | "out_of_memory" | "oom" => Self::Kind(EVENT_OUT_OF_MEMORY),
            "pin.internal_exception" | "pin_internal_exception" | "internal_exception" => {
                Self::Kind(EVENT_PIN_INTERNAL_EXCEPTION)
            }
            _ => return None,
        })
    }

    pub fn event_type(self) -> &'static str {
        match self {
            Self::Exception => "exception",
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
            Self::Kind(_) => "unknown",
        }
    }

    pub fn matches(self, event: &EventRecord) -> bool {
        match self {
            Self::Kind(kind) => event.kind == kind,
            Self::Exception => {
                event.kind == EVENT_CONTEXT_CHANGE && event.arg0 == CONTEXT_CHANGE_EXCEPTION
            }
        }
    }

    pub fn is_sticky(self) -> bool {
        matches!(
            self,
            Self::Kind(EVENT_PROCESS_START) | Self::Kind(EVENT_PROCESS_EXIT)
        )
    }

    pub fn is_priority(self) -> bool {
        matches!(
            self,
            Self::Kind(EVENT_THREAD_START)
                | Self::Kind(EVENT_THREAD_EXIT)
                | Self::Kind(EVENT_PROCESS_START)
                | Self::Kind(EVENT_PROCESS_EXIT)
                | Self::Kind(EVENT_SMC)
                | Self::Kind(EVENT_PIN_DETACH)
                | Self::Kind(EVENT_PIN_ATTACH)
                | Self::Kind(EVENT_OUT_OF_MEMORY)
                | Self::Kind(EVENT_PIN_INTERNAL_EXCEPTION)
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
    /// Sticky process events are replayed once to handlers registered after
    /// the native edge.  This flag is per subscription, so adding a second
    /// handler later still receives the current lifecycle state.
    pub sticky_delivered: bool,
}

impl EventSubscription {
    pub fn new(selector: EventSelector, callback: Py<PyAny>, once: bool) -> (u64, Self) {
        let id = NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed);
        (
            id,
            Self {
                selector,
                callback,
                once,
                order: id,
                sticky_delivered: false,
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
        EventSelector::Exception => {
            row.set_item("reason", event.arg0)?;
            row.set_item("code", event.arg1)?;
            row.set_item("ip", event.arg2)?;
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
        EventSelector::Kind(EVENT_PROCESS_EXIT) => {
            row.set_item("phase", "exiting")?;
            row.set_item("exit_code", event.arg0 as i64)?;
            row.set_item(
                "source",
                if event.arg1 == 1 {
                    "exit_api"
                } else {
                    "prepare_fini"
                },
            )?;
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
        EventSelector::Kind(EVENT_MODULE_LOAD) => {
            row.set_item("base", event.arg0)?;
            row.set_item("end", event.arg1)?;
            row.set_item("is_main", event.arg2 != 0)?;
            if let Some(name) = module_name {
                row.set_item("name", name)?;
            }
        }
        EventSelector::Kind(EVENT_MODULE_UNLOAD) => {
            row.set_item("base", event.arg0)?;
            if let Some(name) = module_name {
                row.set_item("name", name)?;
            }
        }
        EventSelector::Kind(EVENT_SYSCALL) => {
            let phase = if event.arg1 == 0 { "enter" } else { "exit" };
            row.set_item("number", event.arg0)?;
            row.set_item("phase", phase)?;
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
        EventSelector::Kind(EVENT_CONTEXT_CHANGE) => {
            row.set_item("reason", event.arg0)?;
            row.set_item("info", event.arg1 as i64)?;
            row.set_item("ip", event.arg2)?;
        }
        EventSelector::Kind(EVENT_MEMORY) => {
            row.set_item("memory_address", event.arg0)?;
            row.set_item("size", event.arg1)?;
            row.set_item("access", event.arg2)?;
        }
        EventSelector::Kind(EVENT_INSTRUCTION_DECODE) => {
            row.set_item("size", event.arg0)?;
            row.set_item("category", event.arg1)?;
            row.set_item("extension", event.arg2)?;
            row.set_item("opcode", event.arg3)?;
            row.set_item("memory_operand_count", event.arg4)?;
            row.set_item("has_fall_through", event.arg5 & 1 != 0)?;
            row.set_item("is_branch", event.arg5 & (1 << 1) != 0)?;
            row.set_item("is_call", event.arg5 & (1 << 2) != 0)?;
            row.set_item("is_return", event.arg5 & (1 << 3) != 0)?;
            row.set_item("is_syscall", event.arg5 & (1 << 4) != 0)?;
        }
        EventSelector::Kind(EVENT_BRANCH_EDGE) => {
            row.set_item("target", event.arg0)?;
            row.set_item("taken", event.arg1 != 0)?;
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
        for name in [
            "code.smc",
            "pin.detach",
            "pin.attach",
            "memory.oom",
            "pin.internal_exception",
        ] {
            let selector = EventSelector::parse(name).expect("public event must parse");
            assert!(selector.is_priority());
            assert!(PUBLIC_EVENT_NAMES.contains(&name));
        }
        assert!(EventSelector::parse("code.smc")
            .expect("SMC selector")
            .requires_smc_registration());
        assert!(!EventSelector::parse("thread.start")
            .expect("thread selector")
            .requires_smc_registration());
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
}
