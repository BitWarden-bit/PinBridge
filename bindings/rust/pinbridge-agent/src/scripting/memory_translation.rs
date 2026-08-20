//! Python-owned memory-address mappings executed by Pin's native callback.
//!
//! The callback is process-global and hot. Python only replaces declarative
//! specs on the scripting thread; application threads read one immutable
//! snapshot without locks, allocation, RPC, or GIL access.

use super::{with_registry, STATE_RUNNING};
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

pub const MAX_MAPPINGS: usize = 64;
pub const MAX_THREADS: usize = 64;
pub const MAX_INSTRUCTION_RANGES: usize = 64;
pub const OP_LOAD: u32 = 1 << PB_PIN_MEMOP_LOAD;
pub const OP_STORE: u32 = 1 << PB_PIN_MEMOP_STORE;
pub const OP_ALL: u32 = OP_LOAD | OP_STORE;

#[derive(Clone)]
pub struct Mapping {
    pub source_start: u64,
    pub source_end: u64,
    pub target_start: u64,
}

#[derive(Clone)]
pub struct Spec {
    pub mappings: Vec<Mapping>,
    pub threads: Vec<u32>,
    pub instruction_ranges: Vec<(u64, u64)>,
    pub operations: u32,
    pub include_pin: bool,
}

struct NativeRule {
    source_start: u64,
    source_end: u64,
    target_start: u64,
    threads: Vec<u32>,
    instruction_ranges: Vec<(u64, u64)>,
    operations: u32,
    include_pin: bool,
}

impl NativeRule {
    #[inline]
    fn translate(&self, info: &PbMemoryTransInfo) -> Option<u64> {
        if info.is_from_pin != 0 && !self.include_pin {
            return None;
        }
        if info.memory_operation >= u32::BITS
            || self.operations & (1u32 << info.memory_operation) == 0
        {
            return None;
        }
        if !self.threads.is_empty() && self.threads.binary_search(&info.thread_id).is_err() {
            return None;
        }
        if !self.instruction_ranges.is_empty()
            && !self.instruction_ranges.iter().any(|&(start, end)| {
                info.instruction_pointer >= start && info.instruction_pointer < end
            })
        {
            return None;
        }
        let access_end = info.address.checked_add(info.size)?;
        if info.address < self.source_start || access_end > self.source_end {
            return None;
        }
        self.target_start
            .checked_add(info.address - self.source_start)
    }
}

struct NativePolicy {
    rules: Vec<NativeRule>,
}

static POLICY: AtomicPtr<NativePolicy> = AtomicPtr::new(core::ptr::null_mut());
static RETIRED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
static GENERATION: AtomicU64 = AtomicU64::new(0);
static SCRATCH_REG0: AtomicU32 = AtomicU32::new(PB_REG_INVALID_);
static SCRATCH_REG1: AtomicU32 = AtomicU32::new(PB_REG_INVALID_);

fn policy() -> &'static NativePolicy {
    let snapshot = POLICY.load(Ordering::Acquire);
    if snapshot.is_null() {
        static EMPTY: NativePolicy = NativePolicy { rules: Vec::new() };
        &EMPTY
    } else {
        unsafe { &*snapshot }
    }
}

fn publish_native(policy: NativePolicy) -> u64 {
    let replacement = Box::into_raw(Box::new(policy));
    let previous = POLICY.swap(replacement, Ordering::AcqRel);
    if !previous.is_null() {
        // A Pin callback may still have borrowed the old snapshot.
        RETIRED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(previous as usize);
    }
    GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "C" fn on_translate(
    info: *const PbMemoryTransInfo,
    _user_data: *mut core::ffi::c_void,
) -> u64 {
    let Some(info) = info.as_ref() else {
        return 0;
    };
    for rule in &policy().rules {
        if let Some(translated) = rule.translate(info) {
            return translated;
        }
    }
    info.address
}

unsafe extern "C" fn on_rewrite(
    instruction_address: u64,
    thread_id: u32,
    memory_address: u64,
    size: u32,
    memory_operation: u32,
    _user_data: *mut core::ffi::c_void,
) -> u64 {
    let info = PbMemoryTransInfo {
        address: memory_address,
        size: size as u64,
        instruction_pointer: instruction_address,
        thread_id,
        memory_operation,
        is_atomic: 0,
        is_rmw: 0,
        is_prefetch: 0,
        is_from_pin: 0,
        reserved: 0,
    };
    for rule in &policy().rules {
        if let Some(translated) = rule.translate(&info) {
            return translated;
        }
    }
    memory_address
}

#[inline]
fn wants_instruction(address: u64) -> bool {
    policy().rules.iter().any(|rule| {
        rule.instruction_ranges.is_empty()
            || rule
                .instruction_ranges
                .iter()
                .any(|&(start, end)| address >= start && address < end)
    })
}

pub unsafe fn instrument(ins: PbInsHandle, address: u64) {
    if !wants_instruction(address) {
        return;
    }
    let reg0 = SCRATCH_REG0.load(Ordering::Relaxed);
    let reg1 = SCRATCH_REG1.load(Ordering::Relaxed);
    if reg0 == PB_REG_INVALID_ || reg1 == PB_REG_INVALID_ {
        return;
    }
    let _ = pb_ins_insert_memory_address_translation(
        ins,
        Some(on_rewrite),
        core::ptr::null_mut(),
        reg0,
        reg1,
    );
}

fn register_callback() -> PbStatus {
    unsafe {
        let mut reg0 = PB_REG_INVALID_;
        let mut reg1 = PB_REG_INVALID_;
        let status = pb_pin_claim_tool_register(&mut reg0);
        if status != PB_OK {
            return status;
        }
        let status = pb_pin_claim_tool_register(&mut reg1);
        if status != PB_OK {
            return status;
        }
        if reg0 == PB_REG_INVALID_ || reg1 == PB_REG_INVALID_ || reg0 == reg1 {
            return PB_ERR_OUT_OF_MEMORY;
        }
        SCRATCH_REG0.store(reg0, Ordering::Release);
        SCRATCH_REG1.store(reg1, Ordering::Release);
        pb_pin_add_memory_address_trans_function(Some(on_translate), core::ptr::null_mut())
    }
}

/// Installs Pin's one process-global callback before the application starts.
/// With no Python rules the callback is an identity mapping.
pub fn initialize() -> PbStatus {
    publish_native(NativePolicy { rules: Vec::new() });
    register_callback()
}

/// Detach clears Pin callbacks and tool-register claims but not the Rust
/// policy snapshot. Claim fresh registers and reconnect the existing rules.
pub fn reregister_after_attach() -> PbStatus {
    register_callback()
}

fn instrumentation_scope(policy: &NativePolicy) -> (bool, Vec<(u64, u64)>) {
    let global = policy
        .rules
        .iter()
        .any(|rule| rule.instruction_ranges.is_empty());
    let mut ranges = policy
        .rules
        .iter()
        .flat_map(|rule| rule.instruction_ranges.iter().copied())
        .collect::<Vec<_>>();
    ranges.sort_unstable();
    ranges.dedup();
    (global, ranges)
}

/// Rebuilds the process-global native snapshot from all live plugin specs.
pub fn publish() -> Result<u64, PbStatus> {
    let (old_global, mut flush_ranges) = instrumentation_scope(policy());
    let mut specs = with_registry(|registry| {
        registry
            .values()
            .filter(|plugin| plugin.state == STATE_RUNNING)
            .filter_map(|plugin| {
                plugin
                    .memory_translation
                    .as_ref()
                    .map(|spec| (plugin.name.clone(), spec.clone()))
            })
            .collect::<Vec<_>>()
    });
    specs.sort_by(|left, right| left.0.cmp(&right.0));

    let total = specs
        .iter()
        .map(|(_, spec)| spec.mappings.len())
        .sum::<usize>();
    if total > MAX_MAPPINGS {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }
    let mut occupied = Vec::<(u64, u64)>::with_capacity(total);
    let mut rules = Vec::with_capacity(total);
    for (_, spec) in specs {
        if spec.operations == 0
            || spec.operations & !OP_ALL != 0
            || spec.threads.len() > MAX_THREADS
            || spec.instruction_ranges.len() > MAX_INSTRUCTION_RANGES
        {
            return Err(PB_ERR_INVALID_ARGUMENT);
        }
        for mapping in spec.mappings {
            if mapping.source_start >= mapping.source_end
                || mapping
                    .target_start
                    .checked_add(mapping.source_end - mapping.source_start)
                    .is_none()
                || occupied
                    .iter()
                    .any(|&(start, end)| mapping.source_start < end && start < mapping.source_end)
            {
                return Err(PB_ERR_INVALID_ARGUMENT);
            }
            occupied.push((mapping.source_start, mapping.source_end));
            rules.push(NativeRule {
                source_start: mapping.source_start,
                source_end: mapping.source_end,
                target_start: mapping.target_start,
                threads: spec.threads.clone(),
                instruction_ranges: spec.instruction_ranges.clone(),
                operations: spec.operations,
                include_pin: spec.include_pin,
            });
        }
    }
    let replacement = NativePolicy { rules };
    let (new_global, new_ranges) = instrumentation_scope(&replacement);
    flush_ranges.extend(new_ranges);
    flush_ranges.sort_unstable();
    flush_ranges.dedup();
    let generation = publish_native(replacement);
    unsafe {
        if old_global || new_global {
            let status = pb_pin_remove_instrumentation();
            if status != PB_OK {
                return Err(status);
            }
        } else {
            for (start, end) in flush_ranges {
                let status = pb_pin_remove_instrumentation_in_range(start, end);
                if status != PB_OK {
                    return Err(status);
                }
            }
        }
    }
    Ok(generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> PbMemoryTransInfo {
        PbMemoryTransInfo {
            address: 0x1020,
            size: 8,
            instruction_pointer: 0x2050,
            thread_id: 3,
            memory_operation: PB_PIN_MEMOP_LOAD,
            is_atomic: 0,
            is_rmw: 0,
            is_prefetch: 0,
            is_from_pin: 0,
            reserved: 0,
        }
    }

    #[test]
    fn native_translation_honors_source_thread_ip_operation_and_origin() {
        let rule = NativeRule {
            source_start: 0x1000,
            source_end: 0x1100,
            target_start: 0x4000,
            threads: vec![3],
            instruction_ranges: vec![(0x2000, 0x2100)],
            operations: OP_LOAD,
            include_pin: false,
        };
        let mut event = info();
        assert_eq!(rule.translate(&event), Some(0x4020));
        event.thread_id = 4;
        assert_eq!(rule.translate(&event), None);
        event.thread_id = 3;
        event.instruction_pointer = 0x2200;
        assert_eq!(rule.translate(&event), None);
        event.instruction_pointer = 0x2050;
        event.memory_operation = PB_PIN_MEMOP_STORE;
        assert_eq!(rule.translate(&event), None);
        event.memory_operation = PB_PIN_MEMOP_LOAD;
        event.is_from_pin = 1;
        assert_eq!(rule.translate(&event), None);
    }

    #[test]
    fn native_translation_rejects_access_crossing_mapping_end() {
        let rule = NativeRule {
            source_start: 0x1000,
            source_end: 0x1030,
            target_start: 0x2000,
            threads: Vec::new(),
            instruction_ranges: Vec::new(),
            operations: OP_ALL,
            include_pin: false,
        };
        let mut event = info();
        event.address = 0x102f;
        event.size = 2;
        assert_eq!(rule.translate(&event), None);
    }
}
