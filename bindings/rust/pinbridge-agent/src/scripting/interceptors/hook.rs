//! Synchronous function Hook entry/return interception.

use super::super::decisions::{self, DecisionSelector, PythonDecisionGuard};
use super::super::{
    output, with_plugin_context, with_registry, with_registry_mut, STATE_ERROR, STATE_RUNNING,
};
use super::{extract_word, publish_interests, response_set, sort_handlers, Handler};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

fn parse_response(
    value: &Bound<'_, PyAny>,
    request: &crate::sync_intercept::InterceptRequest,
) -> Result<crate::sync_intercept::InterceptResponse, String> {
    let mut response = crate::sync_intercept::InterceptResponse::EMPTY;
    if value.is_none() {
        return Ok(response);
    }
    let dict = value
        .downcast::<PyDict>()
        .map_err(|_| "Hook interceptor must return None or a dictionary".to_string())?;
    if let Some(action) = dict.get_item("action").map_err(|error| error.to_string())? {
        let action = action
            .extract::<String>()
            .map_err(|_| "Hook action must be a string".to_string())?;
        response.action = match action.trim().to_ascii_lowercase().as_str() {
            "continue" | "proceed" | "resume" => crate::sync_intercept::HOOK_ACTION_CONTINUE,
            "return" | "skip" if request.kind == crate::sync_intercept::HOOK_ENTRY => {
                crate::sync_intercept::HOOK_ACTION_RETURN
            }
            "return" | "skip" => {
                return Err("action='return' is valid only for hook.entry".to_string())
            }
            _ => return Err(format!("unknown Hook action: {action}")),
        };
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
            let value = extract_word(&value, &format!("register {name}"))?;
            response_set(
                &mut response.register_mask,
                &mut response.registers,
                index,
                value,
                &format!("register {name}"),
            )?;
        }
    }
    let stack_values = dict
        .get_item("arguments")
        .map_err(|error| error.to_string())?
        .or(dict
            .get_item("stack_arguments")
            .map_err(|error| error.to_string())?);
    if let Some(stack_values) = stack_values {
        if request.kind != crate::sync_intercept::HOOK_ENTRY {
            return Err("Hook return interceptors cannot patch stack arguments".to_string());
        }
        let stack_values = stack_values
            .downcast::<PyList>()
            .map_err(|_| "arguments must be a list with at most four items".to_string())?;
        if stack_values.len() > crate::sync_intercept::MAX_STACK_ARGUMENTS {
            return Err("arguments accepts at most four items".to_string());
        }
        for (index, value) in stack_values.iter().enumerate() {
            let value = extract_word(&value, &format!("arguments[{index}]"))?;
            response_set(
                &mut response.stack_argument_mask,
                &mut response.stack_arguments,
                index,
                value,
                &format!("arguments[{index}]"),
            )?;
        }
    }
    if let Some(return_value) = dict
        .get_item("return_value")
        .map_err(|error| error.to_string())?
    {
        let return_value = extract_word(&return_value, "return_value")?;
        let return_register = crate::arch::return_reg();
        let index = crate::arch::gp_registers()
            .iter()
            .position(|(_, register)| *register == return_register)
            .ok_or_else(|| "native return register is not available".to_string())?;
        response_set(
            &mut response.register_mask,
            &mut response.registers,
            index,
            return_value,
            "return_value",
        )?;
    }
    Ok(response)
}

fn merge_response(
    aggregate: &mut crate::sync_intercept::InterceptResponse,
    response: &crate::sync_intercept::InterceptResponse,
) -> Result<(), String> {
    if response.action == crate::sync_intercept::HOOK_ACTION_RETURN {
        aggregate.action = response.action;
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
    for index in 0..crate::sync_intercept::MAX_STACK_ARGUMENTS {
        if response.stack_argument_mask & (1u32 << index) != 0 {
            response_set(
                &mut aggregate.stack_argument_mask,
                &mut aggregate.stack_arguments,
                index,
                response.stack_arguments[index],
                &format!("arguments[{index}]"),
            )?;
        }
    }
    Ok(())
}

pub(super) fn dispatch(request: crate::sync_intercept::InterceptRequest) {
    Python::with_gil(|py| {
        let selector = if request.kind == crate::sync_intercept::HOOK_RETURN {
            DecisionSelector::HookReturn
        } else {
            DecisionSelector::HookEntry
        };
        let mut handlers = with_registry(|registry| {
            let mut handlers = Vec::new();
            for (name, plugin) in registry {
                if plugin.state != STATE_RUNNING {
                    continue;
                }
                for (id, subscription) in &plugin.decisions {
                    if subscription.selector != selector
                        || subscription.address != Some(request.address)
                        || subscription
                            .thread_id
                            .map(|thread_id| thread_id != request.thread_id)
                            .unwrap_or(false)
                    {
                        continue;
                    }
                    handlers.push(Handler {
                        plugin: name.clone(),
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
            for (index, (name, _)) in crate::arch::gp_registers().iter().enumerate() {
                if request.register_mask & (1u32 << index) != 0 {
                    let _ = registers.set_item(*name, request.registers[index]);
                }
            }
            let arguments = PyList::new_bound(py, request.stack_arguments);
            let event = PyDict::new_bound(py);
            let event_type = if selector == DecisionSelector::HookReturn {
                "hook.return"
            } else {
                "hook.entry"
            };
            let _ = event.set_item("type", event_type);
            let _ = event.set_item("id", handler.id);
            let _ = event.set_item("generation", request.generation);
            let _ = event.set_item("thread_id", request.thread_id);
            let _ = event.set_item("tid", request.thread_id);
            let _ = event.set_item("address", request.address);
            let _ = event.set_item("addr", request.address);
            let _ = event.set_item("registers", registers);
            let _ = event.set_item("arguments", arguments);
            let result = with_plugin_context(&handler.plugin, || {
                let _guard = PythonDecisionGuard::enter();
                handler
                    .callback
                    .call1(py, (event,))
                    .map_err(|error| error.to_string())
                    .and_then(|value| parse_response(value.bind(py), &request))
                    .and_then(|response| {
                        merge_response(&mut aggregate, &response).map(|_| response)
                    })
            });
            let error = result.err();
            if error.is_some() {
                valid = false;
            }
            let mut released = false;
            with_registry_mut(|registry| {
                if let Some(plugin) = registry.get_mut(&handler.plugin) {
                    plugin.delivered = plugin.delivered.saturating_add(1);
                    if error.is_some() {
                        plugin.state = STATE_ERROR;
                        registry_changed = true;
                    }
                    if handler.once && plugin.decisions.remove(&handler.id).is_some() {
                        released = true;
                        registry_changed = true;
                    }
                }
            });
            if released && decisions::release_hook(request.address) {
                decisions::queue_hook_removal(request.address);
            }
            if let Some(error) = error {
                output::push(
                    &handler.plugin,
                    &format!("pb.intercept({event_type}) failed: {error}"),
                );
                crate::log::line(&format!(
                    "plugin {} {event_type} failed: {error}",
                    handler.plugin
                ));
            }
        }
        if registry_changed {
            publish_interests();
            super::super::publish_list_snapshot();
        }
        if !valid {
            super::super::native_policies::refresh_best_effort("Hook interceptor failed");
            aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        }
        crate::sync_intercept::complete(request.slot, request.generation, aggregate);
    });
}
