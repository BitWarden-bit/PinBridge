//! Pin detach/reattach session ownership.
//!
//! Pin removes callback registrations at detach. Rust policy snapshots,
//! Python plugin state, and Pin internal control threads remain resident, so
//! the attach callback rebuilds every session-scoped registration before Pin
//! resumes instrumented execution. Python is never called from that callback.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use pinbridge_sys::*;

const STATE_ATTACHED: u32 = 0;
const STATE_DETACH_REQUESTED: u32 = 1;
const STATE_DETACHED: u32 = 2;
const STATE_ATTACH_REQUESTED: u32 = 3;
const STATE_ATTACHING: u32 = 4;
const STATE_ATTACH_FAILED: u32 = 5;

static STATE: AtomicU32 = AtomicU32::new(STATE_ATTACHED);
static PROBE_MODE: AtomicBool = AtomicBool::new(false);
static LAST_REGISTRATION_STATUS: AtomicI32 = AtomicI32::new(PB_OK);

#[derive(Clone, Copy, Debug)]
pub struct RegistrationFailure {
    pub component: &'static str,
    pub status: PbStatus,
}

fn checked(component: &'static str, status: PbStatus) -> Result<(), RegistrationFailure> {
    crate::log::line(&format!("session callback {component} -> {status}"));
    if status == PB_OK {
        Ok(())
    } else {
        Err(RegistrationFailure { component, status })
    }
}

/// Registers everything Pin removes at detach. One list owns both initial
/// startup and reattach so the two sessions cannot silently drift apart.
pub fn register_callbacks(reattach: bool) -> Result<(), RegistrationFailure> {
    unsafe {
        if reattach {
            checked("hook.tls", crate::hooks::init())?;
            checked(
                "python.native_policies",
                crate::scripting::reregister_after_attach(),
            )?;
        }

        let mut instrument = PbCallbackHandle { opaque: 0 };
        checked(
            "instruction",
            pb_ins_add_instrument_function(
                Some(crate::engines::on_ins),
                core::ptr::null_mut(),
                &mut instrument,
            ),
        )?;

        let mut fini = PbCallbackHandle { opaque: 0 };
        checked(
            "fini",
            pb_pin_add_fini_function(Some(crate::on_fini), core::ptr::null_mut(), &mut fini),
        )?;
        checked(
            "instrumentation.lifecycle",
            crate::instrumentation_lifecycle::register(),
        )?;
        checked("syscall", crate::syscall_engine::register())?;
        checked("exception", crate::exception::register())?;
        checked("modules", crate::modules::register())?;
        checked("lifecycle", crate::lifecycle::register())?;

        let (oom, detach) = crate::high_priority::register();
        checked("memory.oom", oom)?;
        checked("pin.detach", detach)?;
        checked("child.follow", crate::child_process::init_and_register())?;
        checked("debugger.events", crate::debugger::register())?;

        if reattach {
            checked(
                "code.smc",
                crate::high_priority::reregister_smc_after_attach(),
            )?;
        }
    }
    Ok(())
}

pub fn initialize() -> PbStatus {
    let mut probe = 0u8;
    let status = unsafe { pb_pin_is_probe_mode(&mut probe) };
    if status == PB_OK {
        PROBE_MODE.store(probe != 0, Ordering::Release);
        STATE.store(STATE_ATTACHED, Ordering::Release);
        LAST_REGISTRATION_STATUS.store(PB_OK, Ordering::Release);
    }
    status
}

pub fn state_name() -> &'static str {
    match STATE.load(Ordering::Acquire) {
        STATE_ATTACHED => "attached",
        STATE_DETACH_REQUESTED => "detach_requested",
        STATE_DETACHED => "detached",
        STATE_ATTACH_REQUESTED => "attach_requested",
        STATE_ATTACHING => "attaching",
        STATE_ATTACH_FAILED => "attach_failed",
        _ => "unknown",
    }
}

pub fn last_registration_status() -> PbStatus {
    LAST_REGISTRATION_STATUS.load(Ordering::Acquire)
}

pub fn attach_supported() -> bool {
    PROBE_MODE.load(Ordering::Acquire) || !cfg!(windows)
}

pub fn request_detach() -> PbStatus {
    if STATE
        .compare_exchange(
            STATE_ATTACHED,
            STATE_DETACH_REQUESTED,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return PB_ERR_INVALID_STATE;
    }
    let status = unsafe {
        if PROBE_MODE.load(Ordering::Acquire) {
            pb_pin_detach_probed()
        } else {
            pb_pin_detach()
        }
    };
    if status != PB_OK {
        STATE.store(STATE_ATTACHED, Ordering::Release);
    }
    status
}

pub fn note_detached() {
    STATE.store(STATE_DETACHED, Ordering::Release);
}

pub fn note_application_started() {
    if STATE.load(Ordering::Acquire) == STATE_ATTACHED {
        return;
    }
    // A partially restored session may still have registered the lifecycle
    // callback before a later component failed. Never let that notification
    // overwrite attach_failed with a false healthy state.
    if LAST_REGISTRATION_STATUS.load(Ordering::Acquire) == PB_OK {
        let _ = STATE.compare_exchange(
            STATE_ATTACHING,
            STATE_ATTACHED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

unsafe extern "C" fn on_attach(_user_data: *mut c_void) {
    STATE.store(STATE_ATTACHING, Ordering::Release);
    match register_callbacks(true) {
        Ok(()) => {
            LAST_REGISTRATION_STATUS.store(PB_OK, Ordering::Release);
            crate::log::line("Pin reattach callbacks restored");
        }
        Err(failure) => {
            LAST_REGISTRATION_STATUS.store(failure.status, Ordering::Release);
            STATE.store(STATE_ATTACH_FAILED, Ordering::Release);
            crate::log::line(&format!(
                "Pin reattach failed at {} -> {}",
                failure.component, failure.status
            ));
        }
    }
}

/// Returns PB_ATTACH_INITIATED or PB_ATTACH_FAILED_DETACH on bridge success.
pub fn request_attach() -> Result<PbAttachStatus, PbStatus> {
    if !attach_supported() {
        return Err(PB_ERR_UNSUPPORTED);
    }
    if STATE
        .compare_exchange(
            STATE_DETACHED,
            STATE_ATTACH_REQUESTED,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return Err(PB_ERR_INVALID_STATE);
    }
    let mut attach_status = PB_ATTACH_FAILED_DETACH;
    let status = unsafe {
        if PROBE_MODE.load(Ordering::Acquire) {
            pb_pin_attach_probed(Some(on_attach), core::ptr::null_mut(), &mut attach_status)
        } else {
            pb_pin_attach(Some(on_attach), core::ptr::null_mut(), &mut attach_status)
        }
    };
    if status != PB_OK {
        STATE.store(STATE_DETACHED, Ordering::Release);
        return Err(status);
    }
    if attach_status == PB_ATTACH_FAILED_DETACH {
        STATE.store(STATE_DETACHED, Ordering::Release);
    }
    Ok(attach_status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_jit_reattach_is_reported_as_unsupported() {
        PROBE_MODE.store(false, Ordering::Release);
        assert_eq!(attach_supported(), !cfg!(windows));
    }

    #[test]
    fn public_state_names_are_explicit() {
        for (state, expected) in [
            (STATE_ATTACHED, "attached"),
            (STATE_DETACH_REQUESTED, "detach_requested"),
            (STATE_DETACHED, "detached"),
            (STATE_ATTACH_REQUESTED, "attach_requested"),
            (STATE_ATTACHING, "attaching"),
            (STATE_ATTACH_FAILED, "attach_failed"),
        ] {
            STATE.store(state, Ordering::Release);
            assert_eq!(state_name(), expected);
        }
        STATE.store(STATE_ATTACHED, Ordering::Release);
    }
}
