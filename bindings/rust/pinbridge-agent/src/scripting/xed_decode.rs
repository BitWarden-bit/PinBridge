//! Python-configured inputs for Pin's process-global pre-XED-decode callback.
//!
//! This callback is not a decoded-instruction notification: Pin invokes it
//! before XED decodes an instruction. Python publishes a tiny immutable
//! Boolean policy; decoder threads only perform one atomic load and, when a
//! feature is selected, call the fixed C ABI primitive on the borrowed XED
//! object. The borrowed pointer never leaves the callback.

use super::{with_registry, STATE_RUNNING};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use pinbridge_sys::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    pub cet: Option<bool>,
    pub cldemote: Option<bool>,
    pub mpx: Option<bool>,
}

// Low byte: selected features. High byte: enabled values.
static ACTIVE: AtomicU32 = AtomicU32::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn encode(spec: Spec) -> u32 {
    let mut selected = 0;
    let mut enabled = 0;
    for (value, feature) in [
        (spec.cet, PB_XED_DECODE_FEATURE_CET),
        (spec.cldemote, PB_XED_DECODE_FEATURE_CLDEMOTE),
        (spec.mpx, PB_XED_DECODE_FEATURE_MPX),
    ] {
        if let Some(value) = value {
            selected |= feature;
            if value {
                enabled |= feature;
            }
        }
    }
    selected | (enabled << 8)
}

fn merge_value(current: &mut Option<bool>, incoming: Option<bool>) -> Result<(), PbStatus> {
    if let Some(incoming) = incoming {
        if current.is_some_and(|value| value != incoming) {
            return Err(PB_ERR_INVALID_ARGUMENT);
        }
        *current = Some(incoming);
    }
    Ok(())
}

fn merged_spec() -> Result<Spec, PbStatus> {
    let mut merged = Spec {
        cet: None,
        cldemote: None,
        mpx: None,
    };
    let mut specs = with_registry(|registry| {
        registry
            .values()
            .filter(|plugin| plugin.state == STATE_RUNNING)
            .filter_map(|plugin| plugin.xed_decode.map(|spec| (plugin.name.clone(), spec)))
            .collect::<Vec<_>>()
    });
    specs.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, spec) in specs {
        merge_value(&mut merged.cet, spec.cet)?;
        merge_value(&mut merged.cldemote, spec.cldemote)?;
        merge_value(&mut merged.mpx, spec.mpx)?;
    }
    Ok(merged)
}

unsafe extern "C" fn on_xed_decode(
    decoded_instruction: PbXedDecodedInstHandle,
    _user_data: *mut core::ffi::c_void,
) {
    let active = ACTIVE.load(Ordering::Acquire);
    let selected = active & 0xff;
    if selected == 0 {
        return;
    }
    let enabled = (active >> 8) & selected;
    let _ = pb_xed_decoded_inst_set_features(decoded_instruction, selected, enabled);
}

/// Registers Pin's one pre-decode callback before application execution.
/// With no plugin policy the callback is one atomic load and a return.
pub fn initialize() -> PbStatus {
    ACTIVE.store(0, Ordering::Release);
    unsafe { pb_pin_add_xed_decode_callback_function(Some(on_xed_decode), core::ptr::null_mut()) }
}

/// Publishes the agreement of all live plugins. Conflicting explicit values
/// are rejected rather than allowing plugin load order to change decoding.
pub fn publish() -> Result<u64, PbStatus> {
    let replacement = encode(merged_spec()?);
    let previous = ACTIVE.load(Ordering::Acquire);
    if replacement == previous {
        return Ok(GENERATION.load(Ordering::Acquire));
    }
    ACTIVE.store(replacement, Ordering::Release);
    let status = unsafe { pb_pin_remove_instrumentation() };
    if status != PB_OK {
        ACTIVE.store(previous, Ordering::Release);
        return Err(status);
    }
    Ok(GENERATION.fetch_add(1, Ordering::AcqRel) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_distinguishes_unspecified_disabled_and_enabled() {
        assert_eq!(
            encode(Spec {
                cet: None,
                cldemote: Some(false),
                mpx: Some(true),
            }),
            PB_XED_DECODE_FEATURE_CLDEMOTE
                | PB_XED_DECODE_FEATURE_MPX
                | (PB_XED_DECODE_FEATURE_MPX << 8)
        );
    }

    #[test]
    fn merge_rejects_only_explicit_disagreement() {
        let mut value = None;
        assert_eq!(merge_value(&mut value, None), Ok(()));
        assert_eq!(merge_value(&mut value, Some(true)), Ok(()));
        assert_eq!(merge_value(&mut value, Some(true)), Ok(()));
        assert_eq!(
            merge_value(&mut value, Some(false)),
            Err(PB_ERR_INVALID_ARGUMENT)
        );
    }
}
