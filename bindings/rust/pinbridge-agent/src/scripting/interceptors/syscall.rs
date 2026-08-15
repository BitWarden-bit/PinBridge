//! Synchronous system-call entry/exit interception.

use super::super::decisions::{DecisionSelector, PythonDecisionGuard};
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
        .map_err(|_| "syscall interceptor must return None or a dictionary".to_string())?;
    let entry = request.kind == crate::sync_intercept::SYSCALL_ENTRY;
    if let Some(number) = dict.get_item("number").map_err(|error| error.to_string())? {
        if !entry {
            return Err("number can be changed only at syscall.entry".to_string());
        }
        response.syscall_number_set = true;
        response.syscall_number = extract_word(&number, "number")?;
    }
    if let Some(arguments) = dict
        .get_item("arguments")
        .map_err(|error| error.to_string())?
    {
        if !entry {
            return Err("arguments can be changed only at syscall.entry".to_string());
        }
        let arguments = arguments
            .downcast::<PyList>()
            .map_err(|_| "arguments must be a list with at most six items".to_string())?;
        if arguments.len() > crate::sync_intercept::MAX_SYSCALL_ARGUMENTS {
            return Err("arguments accepts at most six items".to_string());
        }
        for (index, value) in arguments.iter().enumerate() {
            response.syscall_argument_mask |= 1u32 << index;
            response.syscall_arguments[index] =
                extract_word(&value, &format!("arguments[{index}]"))?;
        }
    }
    if let Some(return_value) = dict
        .get_item("return_value")
        .map_err(|error| error.to_string())?
    {
        if entry {
            return Err("return_value can be changed only at syscall.exit".to_string());
        }
        response.syscall_return_set = true;
        response.syscall_return = extract_word(&return_value, "return_value")?;
    }
    if let Some(errno) = dict.get_item("errno").map_err(|error| error.to_string())? {
        if entry {
            return Err("errno can be changed only at syscall.exit".to_string());
        }
        response.syscall_errno_set = true;
        response.syscall_errno = extract_word(&errno, "errno")?;
    }
    Ok(response)
}

fn merge_optional_word(
    aggregate_set: &mut bool,
    aggregate: &mut u64,
    incoming_set: bool,
    incoming: u64,
    field: &str,
) -> Result<(), String> {
    if !incoming_set {
        return Ok(());
    }
    if *aggregate_set && *aggregate != incoming {
        return Err(format!("conflicting values for {field}"));
    }
    *aggregate_set = true;
    *aggregate = incoming;
    Ok(())
}

fn merge_response(
    aggregate: &mut crate::sync_intercept::InterceptResponse,
    response: &crate::sync_intercept::InterceptResponse,
) -> Result<(), String> {
    merge_optional_word(
        &mut aggregate.syscall_number_set,
        &mut aggregate.syscall_number,
        response.syscall_number_set,
        response.syscall_number,
        "number",
    )?;
    merge_optional_word(
        &mut aggregate.syscall_return_set,
        &mut aggregate.syscall_return,
        response.syscall_return_set,
        response.syscall_return,
        "return_value",
    )?;
    merge_optional_word(
        &mut aggregate.syscall_errno_set,
        &mut aggregate.syscall_errno,
        response.syscall_errno_set,
        response.syscall_errno,
        "errno",
    )?;
    for index in 0..crate::sync_intercept::MAX_SYSCALL_ARGUMENTS {
        if response.syscall_argument_mask & (1u32 << index) != 0 {
            response_set(
                &mut aggregate.syscall_argument_mask,
                &mut aggregate.syscall_arguments,
                index,
                response.syscall_arguments[index],
                &format!("arguments[{index}]"),
            )?;
        }
    }
    Ok(())
}

pub(super) fn dispatch(request: crate::sync_intercept::InterceptRequest) {
    Python::with_gil(|py| {
        let selector = if request.kind == crate::sync_intercept::SYSCALL_EXIT {
            DecisionSelector::SyscallExit
        } else {
            DecisionSelector::SyscallEntry
        };
        let mut handlers = with_registry(|registry| {
            let mut handlers = Vec::new();
            for (name, plugin) in registry {
                if plugin.state != STATE_RUNNING {
                    continue;
                }
                for (id, subscription) in &plugin.decisions {
                    if subscription.selector != selector
                        || subscription
                            .thread_id
                            .map(|thread_id| thread_id != request.thread_id)
                            .unwrap_or(false)
                        || subscription
                            .numbers
                            .as_ref()
                            .map(|numbers| !numbers.contains(&(request.syscall_number as u32)))
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
            let event = PyDict::new_bound(py);
            let event_type = if selector == DecisionSelector::SyscallExit {
                "syscall.exit"
            } else {
                "syscall.entry"
            };
            let _ = event.set_item("type", event_type);
            let _ = event.set_item("id", handler.id);
            let _ = event.set_item("generation", request.generation);
            let _ = event.set_item("thread_id", request.thread_id);
            let _ = event.set_item("tid", request.thread_id);
            let _ = event.set_item("address", request.address);
            let _ = event.set_item("addr", request.address);
            let _ = event.set_item("number", request.syscall_number);
            let _ = event.set_item("standard", request.syscall_standard);
            let _ = event.set_item(
                "arguments",
                PyList::new_bound(py, request.syscall_arguments),
            );
            let _ = event.set_item("return_value", request.syscall_return);
            let _ = event.set_item("errno", request.syscall_errno);
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
            super::super::instrumentation::publish_best_effort("syscall interceptor failed");
            aggregate = crate::sync_intercept::InterceptResponse::EMPTY;
        }
        crate::sync_intercept::complete(request.slot, request.generation, aggregate);
    });
}
