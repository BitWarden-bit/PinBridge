//! Synchronous exception-context takeover.

use super::super::decisions::{DecisionSelector, PythonDecisionGuard};
use super::super::{
    output, with_plugin_context, with_registry, with_registry_mut, STATE_ERROR, STATE_RUNNING,
};
use super::{extract_word, publish_interests, response_set, sort_handlers, Handler};
use pyo3::prelude::*;
use pyo3::types::PyDict;

const MAX_INTERCEPT_RETURN_BYTES: usize = 4096;

fn bounded_return(value: &Bound<'_, PyAny>) -> String {
    let mut rendered = value
        .repr()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "<return repr failed>".to_string());
    if rendered.len() <= MAX_INTERCEPT_RETURN_BYTES {
        return rendered;
    }
    let suffix = "…<truncated>";
    let mut end = MAX_INTERCEPT_RETURN_BYTES.saturating_sub(suffix.len());
    while !rendered.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    rendered.truncate(end);
    rendered.push_str(suffix);
    rendered
}

fn parse_response(
    value: &Bound<'_, PyAny>,
) -> Result<crate::sync_intercept::InterceptResponse, String> {
    let mut response = crate::sync_intercept::InterceptResponse::EMPTY;
    if value.is_none() {
        return Ok(response);
    }
    let dict = value
        .downcast::<PyDict>()
        .map_err(|_| "exception.handle must return None or a dictionary".to_string())?;
    let Some(registers) = dict
        .get_item("registers")
        .map_err(|error| error.to_string())?
    else {
        return Ok(response);
    };
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
    Ok(response)
}

fn merge_response(
    aggregate: &mut crate::sync_intercept::InterceptResponse,
    response: &crate::sync_intercept::InterceptResponse,
) -> Result<(), String> {
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
        let mut handlers = with_registry(|registry| {
            let mut handlers = Vec::new();
            for (name, plugin) in registry {
                if plugin.state != STATE_RUNNING {
                    continue;
                }
                for (id, subscription) in &plugin.decisions {
                    if subscription.selector != DecisionSelector::ExceptionHandle
                        || subscription
                            .thread_id
                            .map(|thread_id| thread_id != request.thread_id)
                            .unwrap_or(false)
                        || subscription
                            .codes
                            .as_ref()
                            .map(|codes| !codes.contains(&request.exception_code))
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
            let from_registers = PyDict::new_bound(py);
            let registers = PyDict::new_bound(py);
            for (index, (name, _)) in crate::arch::gp_registers().iter().enumerate() {
                if request.source_register_mask & (1u32 << index) != 0 {
                    let _ = from_registers.set_item(*name, request.source_registers[index]);
                }
                if request.register_mask & (1u32 << index) != 0 {
                    let _ = registers.set_item(*name, request.registers[index]);
                }
            }
            let event = PyDict::new_bound(py);
            let _ = event.set_item("type", "exception.handle");
            let _ = event.set_item("id", handler.id);
            let _ = event.set_item("generation", request.generation);
            let _ = event.set_item("thread_id", request.thread_id);
            let _ = event.set_item("tid", request.thread_id);
            let _ = event.set_item("address", request.address);
            let _ = event.set_item("addr", request.address);
            let _ = event.set_item("reason", request.exception_reason);
            let _ = event.set_item("code", request.exception_code);
            let _ = event.set_item("from_registers", from_registers);
            let _ = event.set_item("registers", registers);
            let callback_result = with_plugin_context(&handler.plugin, || {
                let _guard = PythonDecisionGuard::enter();
                handler
                    .callback
                    .call1(py, (event,))
                    .map_err(|error| error.to_string())
            });
            let (result, last_return) = match callback_result {
                Ok(value) => {
                    let rendered = bounded_return(value.bind(py));
                    let result = parse_response(value.bind(py)).and_then(|response| {
                        merge_response(&mut aggregate, &response).map(|_| response)
                    });
                    (result, Some(rendered))
                }
                Err(error) => (Err(error), None),
            };
            let error = result.err();
            if error.is_some() {
                valid = false;
            }
            with_registry_mut(|registry| {
                if let Some(plugin) = registry.get_mut(&handler.plugin) {
                    plugin.delivered = plugin.delivered.saturating_add(1);
                    if let Some(binding) = plugin.decisions.get_mut(&handler.id) {
                        binding.last_generation = request.generation;
                        binding.last_return = last_return.clone();
                        binding.last_error = error.clone();
                    }
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
                    &format!("pb.intercept(exception.handle) failed: {error}"),
                );
                crate::log::line(&format!(
                    "plugin {} exception.handle failed: {error}",
                    handler.plugin
                ));
            }
        }
        super::super::publish_list_snapshot();
        if registry_changed {
            publish_interests();
        }
        if !valid {
            super::super::native_policies::refresh_best_effort("exception interceptor failed");
            aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        }
        crate::sync_intercept::complete(request.slot, request.generation, aggregate);
    });
}
