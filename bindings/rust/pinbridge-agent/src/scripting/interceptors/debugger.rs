//! Synchronous decisions for application-debugger events.

use super::super::decisions::{DecisionSelector, PythonDecisionGuard};
use super::super::{
    output, with_plugin_context, with_registry, with_registry_mut, STATE_ERROR, STATE_RUNNING,
};
use super::{extract_word, publish_interests, response_set, sort_handlers, Handler};
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn selector_for(kind: u32) -> Option<DecisionSelector> {
    match kind {
        crate::sync_intercept::DEBUGGER_BREAKPOINT => Some(DecisionSelector::DebuggerBreakpoint),
        crate::sync_intercept::DEBUGGER_SINGLE_STEP => Some(DecisionSelector::DebuggerSingleStep),
        crate::sync_intercept::DEBUGGER_ASYNC_BREAK => Some(DecisionSelector::DebuggerAsyncBreak),
        _ => None,
    }
}

fn event_type(selector: DecisionSelector) -> &'static str {
    match selector {
        DecisionSelector::DebuggerBreakpoint => "debugger.breakpoint",
        DecisionSelector::DebuggerSingleStep => "debugger.single_step",
        DecisionSelector::DebuggerAsyncBreak => "debugger.async_break",
        _ => "debugger.unknown",
    }
}

fn set_pass_decision(
    response: &mut crate::sync_intercept::InterceptResponse,
    pass: bool,
) -> Result<(), String> {
    if response.debugger_pass_set && response.debugger_pass_to_debugger != pass {
        return Err("conflicting pass_to_debugger values".to_string());
    }
    response.debugger_pass_set = true;
    response.debugger_pass_to_debugger = pass;
    Ok(())
}

fn parse_response(
    value: &Bound<'_, PyAny>,
    selector: DecisionSelector,
) -> Result<crate::sync_intercept::InterceptResponse, String> {
    let mut response = crate::sync_intercept::InterceptResponse::EMPTY;
    if value.is_none() {
        return Ok(response);
    }
    let dict = value
        .downcast::<PyDict>()
        .map_err(|_| "debugger interceptor must return None or a dictionary".to_string())?;
    if let Some(pass) = dict
        .get_item("pass_to_debugger")
        .map_err(|error| error.to_string())?
    {
        set_pass_decision(
            &mut response,
            pass.extract::<bool>()
                .map_err(|_| "pass_to_debugger must be bool".to_string())?,
        )?;
    }
    if let Some(action) = dict.get_item("action").map_err(|error| error.to_string())? {
        let action = action
            .extract::<String>()
            .map_err(|_| "debugger action must be a string".to_string())?;
        let pass = match action.trim().to_ascii_lowercase().as_str() {
            "pass" | "stop" | "debugger" => true,
            "squash" | "consume" | "resume" => false,
            _ => return Err(format!("unknown debugger action: {action}")),
        };
        set_pass_decision(&mut response, pass)?;
    }
    if let Some(registers) = dict
        .get_item("registers")
        .map_err(|error| error.to_string())?
    {
        let registers = registers
            .downcast::<PyDict>()
            .map_err(|_| "registers must be a dictionary".to_string())?;
        for (name, value) in registers.iter() {
            let name = name
                .extract::<String>()
                .map_err(|_| "register name must be a string".to_string())?;
            let Some(index) = crate::arch::gp_registers()
                .iter()
                .position(|(candidate, _)| candidate.eq_ignore_ascii_case(&name))
            else {
                return Err(format!(
                    "unknown register for {}: {name}",
                    crate::arch::name()
                ));
            };
            response_set(
                &mut response.register_mask,
                &mut response.registers,
                index,
                extract_word(&value, &format!("register {name}"))?,
                &format!("register {name}"),
            )?;
        }
    }

    validate_response(selector, &response)?;
    Ok(response)
}

fn validate_response(
    selector: DecisionSelector,
    response: &crate::sync_intercept::InterceptResponse,
) -> Result<(), String> {
    if selector == DecisionSelector::DebuggerAsyncBreak
        && response.debugger_pass_set
        && !response.debugger_pass_to_debugger
    {
        return Err("debugger.async_break cannot be squashed by Pin".to_string());
    }
    let ip_index = crate::arch::gp_registers()
        .iter()
        .position(|(_, register)| *register == crate::arch::instr_ptr_reg());
    if matches!(
        selector,
        DecisionSelector::DebuggerBreakpoint | DecisionSelector::DebuggerSingleStep
    ) && ip_index
        .map(|index| response.register_mask & (1u32 << index) != 0)
        .unwrap_or(false)
        && (!response.debugger_pass_set || response.debugger_pass_to_debugger)
    {
        return Err(
            "changing the instruction pointer requires pass_to_debugger=False for breakpoint/single-step"
                .to_string(),
        );
    }
    Ok(())
}

fn merge_response(
    aggregate: &mut crate::sync_intercept::InterceptResponse,
    response: &crate::sync_intercept::InterceptResponse,
) -> Result<(), String> {
    if response.debugger_pass_set {
        set_pass_decision(aggregate, response.debugger_pass_to_debugger)?;
    }
    for index in 0..crate::sync_intercept::MAX_REGISTERS {
        if response.register_mask & (1u32 << index) != 0 {
            response_set(
                &mut aggregate.register_mask,
                &mut aggregate.registers,
                index,
                response.registers[index],
                crate::arch::gp_registers()
                    .get(index)
                    .map(|(name, _)| *name)
                    .unwrap_or("register"),
            )?;
        }
    }
    Ok(())
}

pub(super) fn dispatch(request: crate::sync_intercept::InterceptRequest) {
    Python::with_gil(|py| {
        let Some(selector) = selector_for(request.kind) else {
            crate::sync_intercept::complete(
                request.slot,
                request.generation,
                crate::sync_intercept::InterceptResponse::EMPTY,
            );
            return;
        };
        let name = event_type(selector);
        let mut handlers = with_registry(|registry| {
            let mut handlers = Vec::new();
            for (plugin_name, plugin) in registry {
                if plugin.state != STATE_RUNNING {
                    continue;
                }
                for (id, subscription) in &plugin.decisions {
                    if subscription.selector != selector
                        || subscription
                            .thread_id
                            .map(|thread_id| thread_id != request.thread_id)
                            .unwrap_or(false)
                    {
                        continue;
                    }
                    handlers.push(Handler {
                        plugin: plugin_name.clone(),
                        id: *id,
                        callback: subscription.callback.clone_ref(py),
                        once: subscription.once,
                        order: subscription.order,
                    });
                }
            }
            handlers
        });
        sort_handlers(&mut handlers);

        let mut aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        let mut valid = true;
        let mut registry_changed = false;
        for handler in handlers {
            let registers = PyDict::new_bound(py);
            for (index, (register_name, _)) in crate::arch::gp_registers().iter().enumerate() {
                if request.register_mask & (1u32 << index) != 0 {
                    let _ = registers.set_item(*register_name, request.registers[index]);
                }
            }
            let event = PyDict::new_bound(py);
            let _ = event.set_item("type", name);
            let _ = event.set_item("id", handler.id);
            let _ = event.set_item("generation", request.generation);
            let _ = event.set_item("thread_id", request.thread_id);
            let _ = event.set_item("tid", request.thread_id);
            let _ = event.set_item("address", request.address);
            let _ = event.set_item("addr", request.address);
            let _ = event.set_item("registers", registers);
            let result = with_plugin_context(&handler.plugin, || {
                let _guard = PythonDecisionGuard::enter();
                handler
                    .callback
                    .call1(py, (event,))
                    .map_err(|error| error.to_string())
                    .and_then(|value| parse_response(value.bind(py), selector))
                    .and_then(|response| {
                        merge_response(&mut aggregate, &response).map(|_| response)
                    })
            });
            let error = result.err();
            if error.is_some() {
                valid = false;
            }
            with_registry_mut(|registry| {
                if let Some(plugin) = registry.get_mut(&handler.plugin) {
                    plugin.delivered = plugin.delivered.saturating_add(1);
                    if error.is_some() {
                        plugin.state = STATE_ERROR;
                        registry_changed = true;
                    }
                    if handler.once && plugin.decisions.remove(&handler.id).is_some() {
                        registry_changed = true;
                    }
                }
            });
            if let Some(error) = error {
                output::push(
                    &handler.plugin,
                    &format!("pb.intercept({name}) failed: {error}"),
                );
                crate::log::line(&format!("plugin {} {name} failed: {error}", handler.plugin));
            }
        }
        if registry_changed {
            publish_interests();
            super::super::publish_list_snapshot();
        }
        if !valid {
            // Empty means no register patches and native fail-open behavior:
            // pass the event to the debugger.
            super::super::native_policies::refresh_best_effort("debugger interceptor failed");
            aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        }
        crate::sync_intercept::complete(request.slot, request.generation, aggregate);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_kinds_map_to_distinct_public_selectors() {
        assert_eq!(
            selector_for(crate::sync_intercept::DEBUGGER_BREAKPOINT),
            Some(DecisionSelector::DebuggerBreakpoint)
        );
        assert_eq!(
            selector_for(crate::sync_intercept::DEBUGGER_ASYNC_BREAK),
            Some(DecisionSelector::DebuggerAsyncBreak)
        );
    }

    #[test]
    fn pin_debugger_contract_restrictions_are_enforced() {
        let mut response = crate::sync_intercept::InterceptResponse::EMPTY;
        response.debugger_pass_set = true;
        response.debugger_pass_to_debugger = false;
        assert!(validate_response(DecisionSelector::DebuggerAsyncBreak, &response).is_err());

        response = crate::sync_intercept::InterceptResponse::EMPTY;
        let ip_index = crate::arch::gp_registers()
            .iter()
            .position(|(_, register)| *register == crate::arch::instr_ptr_reg())
            .expect("instruction pointer register");
        response.register_mask = 1u32 << ip_index;
        assert!(validate_response(DecisionSelector::DebuggerBreakpoint, &response).is_err());
        response.debugger_pass_set = true;
        response.debugger_pass_to_debugger = false;
        assert!(validate_response(DecisionSelector::DebuggerBreakpoint, &response).is_ok());
    }

    #[test]
    fn conflicting_debugger_destinations_reject_the_whole_merge() {
        let mut aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        let mut squash = crate::sync_intercept::InterceptResponse::EMPTY;
        squash.debugger_pass_set = true;
        squash.debugger_pass_to_debugger = false;
        merge_response(&mut aggregate, &squash).expect("first decision");

        let mut pass = crate::sync_intercept::InterceptResponse::EMPTY;
        pass.debugger_pass_set = true;
        pass.debugger_pass_to_debugger = true;
        assert!(merge_response(&mut aggregate, &pass).is_err());
    }
}
