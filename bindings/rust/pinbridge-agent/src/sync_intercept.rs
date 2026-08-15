//! Bounded synchronous rendezvous for application-thread interceptors.
//!
//! Pin analysis callbacks never enter Python. A matching callback copies a
//! fixed-size request into one of the slots below and waits on its own Pin
//! semaphore for a bounded time. The scripting thread returns a fixed-size
//! patch; timeout, slot pressure, Python unavailability, or any malformed
//! result conservatively continues with the original context.

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const HOOK_ENTRY: u32 = 1;
pub const HOOK_RETURN: u32 = 2;

pub const HOOK_ACTION_CONTINUE: u32 = 0;
pub const HOOK_ACTION_RETURN: u32 = 1;

const SLOT_COUNT: usize = 16;
pub const MAX_REGISTERS: usize = 18;
pub const MAX_STACK_ARGUMENTS: usize = 4;
const DEFAULT_TIMEOUT_MS: u32 = 2000;
const MAX_TIMEOUT_MS: u32 = 10_000;

const IDLE: u32 = 0;
const WRITING: u32 = 1;
const PENDING: u32 = 2;
const HANDLING: u32 = 3;
const DECIDED: u32 = 4;
const CANCELLED: u32 = 5;

#[derive(Clone, Copy)]
struct HookInterest {
    address: u64,
    kind: u32,
}

static HOOK_INTERESTS: AtomicPtr<Vec<HookInterest>> = AtomicPtr::new(core::ptr::null_mut());
static RETIRED_INTERESTS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct NativeRequest {
    kind: u32,
    thread_id: u32,
    address: u64,
    register_mask: u32,
    registers: [u64; MAX_REGISTERS],
    stack_arguments: [u64; MAX_STACK_ARGUMENTS],
}

impl NativeRequest {
    const EMPTY: Self = Self {
        kind: 0,
        thread_id: 0,
        address: 0,
        register_mask: 0,
        registers: [0; MAX_REGISTERS],
        stack_arguments: [0; MAX_STACK_ARGUMENTS],
    };
}

#[derive(Clone, Copy)]
pub struct HookRequest {
    pub slot: usize,
    pub generation: u64,
    pub kind: u32,
    pub thread_id: u32,
    pub address: u64,
    pub register_mask: u32,
    pub registers: [u64; MAX_REGISTERS],
    pub stack_arguments: [u64; MAX_STACK_ARGUMENTS],
}

#[derive(Clone, Copy)]
pub struct HookResponse {
    pub action: u32,
    pub register_mask: u32,
    pub registers: [u64; MAX_REGISTERS],
    pub stack_argument_mask: u32,
    pub stack_arguments: [u64; MAX_STACK_ARGUMENTS],
}

impl HookResponse {
    pub const EMPTY: Self = Self {
        action: HOOK_ACTION_CONTINUE,
        register_mask: 0,
        registers: [0; MAX_REGISTERS],
        stack_argument_mask: 0,
        stack_arguments: [0; MAX_STACK_ARGUMENTS],
    };
}

struct Slot {
    state: AtomicU32,
    generation: AtomicU64,
    semaphore: AtomicUsize,
    request: core::cell::UnsafeCell<NativeRequest>,
    response: core::cell::UnsafeCell<HookResponse>,
}

unsafe impl Sync for Slot {}

impl Slot {
    const fn new() -> Self {
        Self {
            state: AtomicU32::new(IDLE),
            generation: AtomicU64::new(0),
            semaphore: AtomicUsize::new(0),
            request: core::cell::UnsafeCell::new(NativeRequest::EMPTY),
            response: core::cell::UnsafeCell::new(HookResponse::EMPTY),
        }
    }
}

static SLOTS: [Slot; SLOT_COUNT] = [const { Slot::new() }; SLOT_COUNT];
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);
static TIMEOUT_MS: AtomicU32 = AtomicU32::new(DEFAULT_TIMEOUT_MS);
static COMPLETED: AtomicU64 = AtomicU64::new(0);
static TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicU64 = AtomicU64::new(0);

pub fn init() -> PbStatus {
    let configured = std::env::var("PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1, MAX_TIMEOUT_MS);
    TIMEOUT_MS.store(configured, Ordering::Release);

    for slot in &SLOTS {
        let mut semaphore: PbSemaphoreHandle = core::ptr::null_mut();
        let status = unsafe { pb_pin_semaphore_init(&mut semaphore) };
        if status != PB_OK {
            return status;
        }
        slot.semaphore.store(semaphore as usize, Ordering::Release);
    }
    PB_OK
}

/// Replaces the lock-free Hook interest table. Called only on the scripting
/// thread after its registry changes. Retired snapshots stay allocated: an
/// analysis callback may have loaded the old pointer before being suspended.
pub fn publish_hook_interests(interests: &[(u64, bool)]) {
    let mut snapshot: Vec<HookInterest> = interests
        .iter()
        .filter(|(address, _)| *address != 0)
        .map(|(address, is_return)| HookInterest {
            address: *address,
            kind: if *is_return { HOOK_RETURN } else { HOOK_ENTRY },
        })
        .collect();
    snapshot.sort_unstable_by_key(|interest| (interest.address, interest.kind));
    snapshot.dedup_by_key(|interest| (interest.address, interest.kind));
    let replacement = Box::into_raw(Box::new(snapshot));
    let old = HOOK_INTERESTS.swap(replacement, Ordering::AcqRel);
    if !old.is_null() {
        RETIRED_INTERESTS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(old as usize);
    }
}

#[inline]
fn hook_interested(address: u64, kind: u32) -> bool {
    let snapshot = HOOK_INTERESTS.load(Ordering::Acquire);
    if snapshot.is_null() {
        return false;
    }
    unsafe { &*snapshot }
        .binary_search_by_key(&(address, kind), |interest| {
            (interest.address, interest.kind)
        })
        .is_ok()
}

unsafe fn capture_hook(
    kind: u32,
    address: u64,
    thread_id: u32,
    context: PbContextHandle,
    stack_arguments: [u64; MAX_STACK_ARGUMENTS],
) -> NativeRequest {
    let mut request = NativeRequest {
        kind,
        thread_id,
        address,
        register_mask: 0,
        registers: [0; MAX_REGISTERS],
        stack_arguments,
    };
    for (index, (_, register)) in crate::arch::gp_registers().iter().enumerate() {
        let mut value = 0;
        if pb_pin_get_context_reg(context as PbConstContextHandle, *register, &mut value) == PB_OK {
            request.register_mask |= 1u32 << index;
            request.registers[index] = value;
        }
    }
    request
}

/// Called from a Hook analysis callback. Returns None when the event is not
/// intercepted or a safe decision could not be obtained in time.
pub unsafe fn decide_hook(
    address: u64,
    thread_id: u32,
    is_return: bool,
    context: PbContextHandle,
    stack_arguments: [u64; MAX_STACK_ARGUMENTS],
) -> Option<HookResponse> {
    let kind = if is_return { HOOK_RETURN } else { HOOK_ENTRY };
    if context.is_null() || !crate::scripting::python_ready() || !hook_interested(address, kind) {
        return None;
    }
    let Some(slot_index) = SLOTS.iter().position(|slot| {
        slot.state
            .compare_exchange(IDLE, WRITING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }) else {
        BUSY.fetch_add(1, Ordering::Relaxed);
        return None;
    };
    let slot = &SLOTS[slot_index];
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    slot.generation.store(generation, Ordering::Release);
    *slot.request.get() = capture_hook(kind, address, thread_id, context, stack_arguments);
    *slot.response.get() = HookResponse::EMPTY;
    let semaphore = slot.semaphore.load(Ordering::Acquire) as PbSemaphoreHandle;
    if semaphore.is_null() {
        slot.state.store(IDLE, Ordering::Release);
        return None;
    }
    let _ = pb_pin_semaphore_clear(semaphore);
    slot.state.store(PENDING, Ordering::Release);

    let mut woke = 0u8;
    let _ = pb_pin_semaphore_timed_wait(semaphore, TIMEOUT_MS.load(Ordering::Relaxed), &mut woke);
    loop {
        match slot.state.load(Ordering::Acquire) {
            DECIDED => {
                let response = *slot.response.get();
                COMPLETED.fetch_add(1, Ordering::Relaxed);
                slot.state.store(IDLE, Ordering::Release);
                return Some(response);
            }
            PENDING => {
                if slot
                    .state
                    .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            }
            HANDLING => {
                if slot
                    .state
                    .compare_exchange(HANDLING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            }
            CANCELLED => return None,
            _ => return None,
        }
    }
}

pub fn take_pending() -> Option<HookRequest> {
    for (slot_index, slot) in SLOTS.iter().enumerate() {
        if slot
            .state
            .compare_exchange(PENDING, HANDLING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let request = unsafe { *slot.request.get() };
            return Some(HookRequest {
                slot: slot_index,
                generation: slot.generation.load(Ordering::Acquire),
                kind: request.kind,
                thread_id: request.thread_id,
                address: request.address,
                register_mask: request.register_mask,
                registers: request.registers,
                stack_arguments: request.stack_arguments,
            });
        }
        if slot
            .state
            .compare_exchange(CANCELLED, IDLE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            crate::log::line("synchronous interceptor cancelled request reaped");
        }
    }
    None
}

pub fn complete(slot_index: usize, generation: u64, response: HookResponse) {
    let Some(slot) = SLOTS.get(slot_index) else {
        return;
    };
    if slot.generation.load(Ordering::Acquire) != generation {
        return;
    }
    unsafe {
        *slot.response.get() = response;
    }
    if slot
        .state
        .compare_exchange(HANDLING, DECIDED, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let semaphore = slot.semaphore.load(Ordering::Acquire) as PbSemaphoreHandle;
        if !semaphore.is_null() {
            unsafe {
                let _ = pb_pin_semaphore_set(semaphore);
            }
        }
    } else if slot.state.load(Ordering::Acquire) == CANCELLED {
        slot.state.store(IDLE, Ordering::Release);
    }
}

pub fn pending() -> bool {
    SLOTS
        .iter()
        .any(|slot| matches!(slot.state.load(Ordering::Acquire), PENDING | CANCELLED))
}

/// Applies a validated Python patch to the live Hook context. Returns true
/// when the caller must commit the changed context with execute_at.
pub unsafe fn apply_hook_response(
    context: PbContextHandle,
    response: &HookResponse,
    is_return: bool,
) -> bool {
    let mut changed = false;
    for (index, (_, register)) in crate::arch::gp_registers().iter().enumerate() {
        if response.register_mask & (1u32 << index) != 0
            && pb_pin_set_context_reg(context, *register, response.registers[index]) == PB_OK
        {
            changed = true;
        }
    }
    if !is_return {
        for index in 0..MAX_STACK_ARGUMENTS {
            if response.stack_argument_mask & (1u32 << index) != 0
                && pb_pin_set_context_stack_arg(
                    context,
                    index as u32,
                    response.stack_arguments[index],
                ) == PB_OK
            {
                changed = true;
            }
        }
    }
    changed
}

pub fn response_changes_instruction_pointer(response: &HookResponse) -> bool {
    crate::arch::gp_registers()
        .iter()
        .position(|(_, register)| *register == crate::arch::instr_ptr_reg())
        .map(|index| response.register_mask & (1u32 << index) != 0)
        .unwrap_or(false)
}

/// Implements `action="return"` for an entry Hook by popping the native
/// return address and transferring directly to the caller. The response's
/// register patch (including the return-value register) must be applied first.
pub unsafe fn return_from_hook(context: PbContextHandle) -> bool {
    let mut stack_pointer = 0u64;
    if pb_pin_get_context_reg(
        context as PbConstContextHandle,
        crate::arch::stack_ptr_reg(),
        &mut stack_pointer,
    ) != PB_OK
    {
        return false;
    }
    let width = crate::arch::pointer_width() as usize;
    let mut bytes = [0u8; 8];
    let mut copied = 0u64;
    if pb_pin_safe_copy(
        bytes.as_mut_ptr() as *mut c_void,
        stack_pointer,
        width as u64,
        &mut copied,
    ) != PB_OK
        || copied != width as u64
    {
        return false;
    }
    let return_address = if width == 8 {
        u64::from_le_bytes(bytes)
    } else {
        u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64
    };
    pb_pin_set_context_reg(
        context,
        crate::arch::stack_ptr_reg(),
        stack_pointer + width as u64,
    ) == PB_OK
        && pb_pin_set_context_reg(context, crate::arch::instr_ptr_reg(), return_address) == PB_OK
}

pub fn stats() -> (u64, u64, u64) {
    (
        COMPLETED.load(Ordering::Relaxed),
        TIMEOUTS.load(Ordering::Relaxed),
        BUSY.load(Ordering::Relaxed),
    )
}
