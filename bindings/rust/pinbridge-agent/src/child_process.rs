//! Bounded rendezvous for Pin's synchronous child-follow callback.
//!
//! The Pin callback cannot call Python and the CHILD_PROCESS handle is valid
//! only for that callback. It therefore copies pid/argv into one fixed slot,
//! waits on a Pin semaphore for a bounded time, and returns a conservative
//! "do not follow" decision on every failure path.

use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

const MAX_ARGUMENTS: usize = 64;
const ARGUMENT_BYTES: usize = 8192;
const DEFAULT_TIMEOUT_MS: u32 = 2000;
const MAX_TIMEOUT_MS: u32 = 10_000;
const MAX_PIN_ARGUMENTS: usize = 128;
const PIN_ARGUMENT_BYTES: usize = 32 * 1024;
const AGENT_PORT_FLAG: &[u8] = b"-pb_agent_port";
const PARENT_PORT_FLAG: &[u8] = b"-pb_parent_port";

const IDLE: u32 = 0;
const WRITING: u32 = 1;
const PENDING: u32 = 2;
const HANDLING: u32 = 3;
const DECIDED: u32 = 4;
const CANCELLED: u32 = 5;

struct RequestSlot {
    process_id: u32,
    argc: u32,
    offsets: [u16; MAX_ARGUMENTS],
    lengths: [u16; MAX_ARGUMENTS],
    bytes: [u8; ARGUMENT_BYTES],
}

struct PinCommandSlot {
    argc: u32,
    offsets: [u16; MAX_PIN_ARGUMENTS],
    bytes: [u8; PIN_ARGUMENT_BYTES],
}

impl PinCommandSlot {
    const fn new() -> Self {
        Self {
            argc: 0,
            offsets: [0; MAX_PIN_ARGUMENTS],
            bytes: [0; PIN_ARGUMENT_BYTES],
        }
    }
}

/// Owns a sanitized argv for the one `pb_pin_init` call. The two private
/// PinBridge options are removed before Pin parses its own/tool knobs.
pub struct PreparedToolArguments {
    strings: Vec<CString>,
    pointers: Vec<*mut c_char>,
}

impl PreparedToolArguments {
    pub fn argc(&self) -> c_int {
        self.strings.len() as c_int
    }

    pub fn argv(&mut self) -> *mut *mut c_char {
        self.pointers.as_mut_ptr()
    }
}

impl RequestSlot {
    const fn new() -> Self {
        Self {
            process_id: 0,
            argc: 0,
            offsets: [0; MAX_ARGUMENTS],
            lengths: [0; MAX_ARGUMENTS],
            bytes: [0; ARGUMENT_BYTES],
        }
    }
}

pub struct ChildRequest {
    pub generation: u64,
    pub process_id: u32,
    pub arguments: Vec<Vec<u8>>,
}

static STATE: AtomicU32 = AtomicU32::new(IDLE);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACTIVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static RESULT_FOLLOW: AtomicU32 = AtomicU32::new(0);
static RESPONSE_SEMAPHORE: AtomicUsize = AtomicUsize::new(0);
static TIMEOUT_MS: AtomicU32 = AtomicU32::new(DEFAULT_TIMEOUT_MS);
static TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static DECISIONS: AtomicU64 = AtomicU64::new(0);
static FOLLOWED: AtomicU64 = AtomicU64::new(0);
static REJECTED: AtomicU64 = AtomicU64::new(0);
static CONFIG_FAILURES: AtomicU64 = AtomicU64::new(0);
static CONTROL_PORT_OVERRIDE: AtomicU32 = AtomicU32::new(0);
static PARENT_CONTROL_PORT: AtomicU32 = AtomicU32::new(0);
static BASE_PIN_ARGUMENTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
static mut REQUEST: RequestSlot = RequestSlot::new();
static mut CHILD_PIN_COMMAND: PinCommandSlot = PinCommandSlot::new();

fn parse_port(value: &[u8]) -> Result<u16, &'static str> {
    let text = core::str::from_utf8(value).map_err(|_| "agent port is not ASCII")?;
    let port = text
        .parse::<u16>()
        .map_err(|_| "agent port must be 1..65535")?;
    if port == 0 {
        return Err("agent port must be 1..65535");
    }
    Ok(port)
}

fn sanitize_tool_arguments(
    raw: &[Vec<u8>],
) -> Result<(Vec<Vec<u8>>, Option<u16>, Option<u16>), &'static str> {
    let mut sanitized = Vec::with_capacity(raw.len());
    let mut control_port = None;
    let mut parent_port = None;
    let mut index = 0usize;
    let mut before_application = true;
    while index < raw.len() {
        let argument = raw[index].as_slice();
        if before_application && argument == b"--" {
            before_application = false;
            sanitized.push(raw[index].clone());
            index += 1;
            continue;
        }
        let destination = if before_application && argument == AGENT_PORT_FLAG {
            &mut control_port
        } else if before_application && argument == PARENT_PORT_FLAG {
            &mut parent_port
        } else {
            sanitized.push(raw[index].clone());
            index += 1;
            continue;
        };
        let value = raw
            .get(index + 1)
            .ok_or("agent port option needs a value")?;
        *destination = Some(parse_port(value)?);
        index += 2;
    }
    Ok((sanitized, control_port, parent_port))
}

/// Captures the reusable Pin command line and strips PinBridge-private
/// child-session options before forwarding argv to PIN_Init.
pub unsafe fn prepare_tool_arguments(
    argc: c_int,
    argv: *mut *mut c_char,
) -> Result<PreparedToolArguments, &'static str> {
    if argc <= 0 || argv.is_null() {
        return Err("Pin tool argv is empty");
    }
    let mut raw = Vec::with_capacity(argc as usize);
    for index in 0..argc as usize {
        let pointer = *argv.add(index);
        if pointer.is_null() {
            return Err("Pin tool argv contains null");
        }
        raw.push(CStr::from_ptr(pointer).to_bytes().to_vec());
    }
    let (sanitized, control_port, parent_port) = sanitize_tool_arguments(&raw)?;
    let base_end = sanitized
        .iter()
        .position(|argument| argument.as_slice() == b"--")
        .map(|index| index + 1)
        .ok_or("Pin tool argv has no -- separator")?;
    let _ = BASE_PIN_ARGUMENTS.set(sanitized[..base_end].to_vec());
    CONTROL_PORT_OVERRIDE.store(control_port.unwrap_or(0) as u32, Ordering::Release);
    PARENT_CONTROL_PORT.store(parent_port.unwrap_or(0) as u32, Ordering::Release);

    let strings: Vec<CString> = sanitized
        .into_iter()
        .map(|argument| CString::new(argument).expect("CStr bytes cannot contain NUL"))
        .collect();
    let mut pointers: Vec<*mut c_char> = strings
        .iter()
        .map(|argument| argument.as_ptr() as *mut c_char)
        .collect();
    pointers.push(core::ptr::null_mut());
    Ok(PreparedToolArguments { strings, pointers })
}

pub fn control_port_override() -> Option<u16> {
    let port = CONTROL_PORT_OVERRIDE.load(Ordering::Acquire) as u16;
    (port != 0).then_some(port)
}

pub fn parent_control_port() -> Option<u16> {
    let port = PARENT_CONTROL_PORT.load(Ordering::Acquire) as u16;
    (port != 0).then_some(port)
}

fn push_pin_argument(slot: &mut PinCommandSlot, used: &mut usize, argument: &[u8]) -> bool {
    if slot.argc as usize >= MAX_PIN_ARGUMENTS
        || argument.len().saturating_add(1) > u16::MAX as usize
        || used.saturating_add(argument.len()).saturating_add(1) > PIN_ARGUMENT_BYTES
    {
        return false;
    }
    let index = slot.argc as usize;
    slot.offsets[index] = *used as u16;
    slot.bytes[*used..*used + argument.len()].copy_from_slice(argument);
    *used += argument.len();
    slot.bytes[*used] = 0;
    *used += 1;
    slot.argc += 1;
    true
}

/// Runs on the script thread while the Pin callback is waiting. It prepares
/// a fixed command-line snapshot that the callback can apply without
/// allocating or retaining the borrowed CHILD_PROCESS handle.
fn prepare_child_pin_command(control_port: u16, parent_port: u16) -> bool {
    let Some(base) = BASE_PIN_ARGUMENTS.get() else {
        return false;
    };
    let control_text = control_port.to_string();
    let parent_text = parent_port.to_string();
    unsafe {
        let slot = &mut *core::ptr::addr_of_mut!(CHILD_PIN_COMMAND);
        slot.argc = 0;
        let mut used = 0usize;
        let mut inserted = false;
        for argument in base {
            if argument.as_slice() == b"--" {
                inserted = push_pin_argument(slot, &mut used, AGENT_PORT_FLAG)
                    && push_pin_argument(slot, &mut used, control_text.as_bytes())
                    && push_pin_argument(slot, &mut used, PARENT_PORT_FLAG)
                    && push_pin_argument(slot, &mut used, parent_text.as_bytes())
                    && push_pin_argument(slot, &mut used, b"--");
                break;
            }
            if !push_pin_argument(slot, &mut used, argument) {
                return false;
            }
        }
        inserted
    }
}

unsafe fn apply_child_pin_command(child: PbChildProcessHandle) -> bool {
    let slot = &*core::ptr::addr_of!(CHILD_PIN_COMMAND);
    if slot.argc == 0 || slot.argc as usize > MAX_PIN_ARGUMENTS {
        return false;
    }
    let mut pointers = [core::ptr::null(); MAX_PIN_ARGUMENTS];
    for index in 0..slot.argc as usize {
        pointers[index] = slot.bytes.as_ptr().add(slot.offsets[index] as usize) as *const c_char;
    }
    pb_child_process_set_pin_command_line(child, slot.argc as i32, pointers.as_ptr()) == PB_OK
}

unsafe fn capture_request(child: PbChildProcessHandle) -> bool {
    let slot = &mut *core::ptr::addr_of_mut!(REQUEST);
    let mut process_id = 0u32;
    if pb_child_process_get_id(child, &mut process_id) != PB_OK {
        return false;
    }
    let mut argc = 0i32;
    if pb_child_process_get_command_line_count(child, &mut argc) != PB_OK
        || argc < 0
        || argc as usize > MAX_ARGUMENTS
    {
        return false;
    }

    slot.process_id = process_id;
    slot.argc = argc as u32;
    let mut used = 0usize;
    for index in 0..argc as usize {
        let mut required = 0u64;
        let status = pb_child_process_get_command_line_argument(
            child,
            index as i32,
            core::ptr::null_mut(),
            0,
            &mut required,
        );
        if status != PB_ERR_BUFFER_TOO_SMALL
            || required == 0
            || required > u16::MAX as u64
            || used.saturating_add(required as usize) > ARGUMENT_BYTES
        {
            return false;
        }
        let destination = slot.bytes.as_mut_ptr().add(used) as *mut c_char;
        if pb_child_process_get_command_line_argument(
            child,
            index as i32,
            destination,
            required,
            &mut required,
        ) != PB_OK
        {
            return false;
        }
        slot.offsets[index] = used as u16;
        slot.lengths[index] = (required - 1) as u16;
        used += required as usize;
    }
    true
}

unsafe extern "C" fn on_follow_child(child: PbChildProcessHandle, _user_data: *mut c_void) -> u8 {
    if !crate::scripting::python_ready()
        || STATE
            .compare_exchange(IDLE, WRITING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return 0;
    }
    if !capture_request(child) {
        STATE.store(IDLE, Ordering::Release);
        return 0;
    }

    let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    ACTIVE_GENERATION.store(generation, Ordering::Release);
    RESULT_FOLLOW.store(0, Ordering::Release);
    let semaphore = RESPONSE_SEMAPHORE.load(Ordering::Acquire) as PbSemaphoreHandle;
    if semaphore.is_null() {
        STATE.store(IDLE, Ordering::Release);
        return 0;
    }
    let _ = pb_pin_semaphore_clear(semaphore);
    STATE.store(PENDING, Ordering::Release);

    let mut woke = 0u8;
    let _wait_status =
        pb_pin_semaphore_timed_wait(semaphore, TIMEOUT_MS.load(Ordering::Relaxed), &mut woke);
    loop {
        match STATE.load(Ordering::Acquire) {
            DECIDED => {
                let mut follow = RESULT_FOLLOW.load(Ordering::Acquire) != 0;
                if follow && !apply_child_pin_command(child) {
                    CONFIG_FAILURES.fetch_add(1, Ordering::Relaxed);
                    follow = false;
                }
                DECISIONS.fetch_add(1, Ordering::Relaxed);
                if follow {
                    FOLLOWED.fetch_add(1, Ordering::Relaxed);
                } else {
                    REJECTED.fetch_add(1, Ordering::Relaxed);
                }
                STATE.store(IDLE, Ordering::Release);
                return follow as u8;
            }
            PENDING => {
                if STATE
                    .compare_exchange(PENDING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return 0;
                }
            }
            HANDLING => {
                if STATE
                    .compare_exchange(HANDLING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                    return 0;
                }
            }
            CANCELLED => return 0,
            _ => return 0,
        }
    }
}

pub fn init_and_register() -> PbStatus {
    let configured = std::env::var("PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1, MAX_TIMEOUT_MS);
    TIMEOUT_MS.store(configured, Ordering::Release);

    unsafe {
        let mut semaphore: PbSemaphoreHandle = core::ptr::null_mut();
        let status = pb_pin_semaphore_init(&mut semaphore);
        if status != PB_OK {
            return status;
        }
        RESPONSE_SEMAPHORE.store(semaphore as usize, Ordering::Release);
        let mut callback = PbCallbackHandle { opaque: 0 };
        pb_pin_add_follow_child_process_function(
            Some(on_follow_child),
            core::ptr::null_mut(),
            &mut callback,
        )
    }
}

/// Claims one request for the scripting thread and copies the fixed slot into
/// ordinary owned values. No Pin handle crosses the callback boundary.
pub fn take_pending() -> Option<ChildRequest> {
    if STATE
        .compare_exchange(PENDING, HANDLING, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        if STATE
            .compare_exchange(CANCELLED, IDLE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            crate::log::line("child.follow cancelled request reaped");
        }
        return None;
    }
    unsafe {
        let slot = &*core::ptr::addr_of!(REQUEST);
        let mut arguments = Vec::with_capacity(slot.argc as usize);
        for index in 0..slot.argc as usize {
            let start = slot.offsets[index] as usize;
            let end = start + slot.lengths[index] as usize;
            arguments.push(slot.bytes[start..end].to_vec());
        }
        Some(ChildRequest {
            generation: ACTIVE_GENERATION.load(Ordering::Acquire),
            process_id: slot.process_id,
            arguments,
        })
    }
}

/// Publishes the Python decision unless the native callback already timed
/// out. A cancelled request owns the slot until this function retires it.
pub fn complete(generation: u64, follow: bool, control_port: u16, parent_port: u16) {
    if ACTIVE_GENERATION.load(Ordering::Acquire) != generation {
        return;
    }
    let command_ready = !follow || prepare_child_pin_command(control_port, parent_port);
    if follow && !command_ready {
        CONFIG_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    let follow = follow && command_ready;
    RESULT_FOLLOW.store(follow as u32, Ordering::Release);
    if STATE
        .compare_exchange(HANDLING, DECIDED, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let semaphore = RESPONSE_SEMAPHORE.load(Ordering::Acquire) as PbSemaphoreHandle;
        if !semaphore.is_null() {
            unsafe {
                let _ = pb_pin_semaphore_set(semaphore);
            }
        }
    } else if STATE.load(Ordering::Acquire) == CANCELLED {
        STATE.store(IDLE, Ordering::Release);
    }
}

pub fn pending() -> bool {
    matches!(STATE.load(Ordering::Acquire), PENDING | CANCELLED)
}

pub fn timeout_count() -> u64 {
    TIMEOUTS.load(Ordering::Relaxed)
}

pub fn decision_counts() -> (u64, u64, u64) {
    (
        DECISIONS.load(Ordering::Relaxed),
        FOLLOWED.load(Ordering::Relaxed),
        REJECTED.load(Ordering::Relaxed),
    )
}

pub fn configuration_failure_count() -> u64 {
    CONFIG_FAILURES.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<Vec<u8>> {
        values
            .iter()
            .map(|value| value.as_bytes().to_vec())
            .collect()
    }

    #[test]
    fn private_child_session_options_are_removed_before_pin_init() {
        let raw = arguments(&[
            "pin.exe",
            "-follow_execv",
            "-t",
            "agent.dll",
            "-pb_agent_port",
            "43123",
            "-pb_parent_port",
            "43122",
            "--",
            "target.exe",
            "-pb_agent_port",
            "not-a-tool-option",
        ]);
        let (sanitized, control, parent) = sanitize_tool_arguments(&raw).unwrap();
        assert_eq!(control, Some(43123));
        assert_eq!(parent, Some(43122));
        assert_eq!(
            sanitized,
            arguments(&[
                "pin.exe",
                "-follow_execv",
                "-t",
                "agent.dll",
                "--",
                "target.exe",
                "-pb_agent_port",
                "not-a-tool-option",
            ])
        );
    }

    #[test]
    fn private_child_session_ports_must_be_valid() {
        assert!(sanitize_tool_arguments(&arguments(&[
            "pin.exe",
            "-t",
            "agent.dll",
            "-pb_agent_port",
            "0",
            "--",
        ]))
        .is_err());
        assert!(sanitize_tool_arguments(&arguments(&[
            "pin.exe",
            "-t",
            "agent.dll",
            "-pb_parent_port",
            "70000",
            "--",
        ]))
        .is_err());
    }
}
