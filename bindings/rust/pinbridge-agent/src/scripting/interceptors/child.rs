//! Synchronous child-process follow decisions.

use super::super::decisions::{DecisionSelector, PythonDecisionGuard};
use super::super::{
    output, with_plugin_context, with_registry, with_registry_mut, STATE_ERROR, STATE_RUNNING,
};
use super::{sort_handlers, Handler};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

fn reserve_child_control_port() -> Option<std::net::TcpListener> {
    std::net::TcpListener::bind(("127.0.0.1", 0)).ok()
}

fn parse_follow(value: &Bound<'_, PyAny>) -> Result<bool, String> {
    if let Ok(follow) = value.extract::<bool>() {
        return Ok(follow);
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        let follow = dict
            .get_item("follow")
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "child.follow result dictionary needs 'follow'".to_string())?;
        return follow
            .extract::<bool>()
            .map_err(|_| "child.follow result 'follow' must be bool".to_string());
    }
    Err("child.follow callback must return bool or {'follow': bool}".to_string())
}

/// The native follow-child callback may be waiting while it owns Pin state
/// needed by ordinary target RPCs. Python can compute and log; the decision
/// guard makes target RPCs fail fast instead of deadlocking the rendezvous.
pub(super) fn dispatch() {
    let Some(request) = crate::child_process::take_pending() else {
        return;
    };
    Python::with_gil(|py| {
        // Retain the listener throughout Python decision handling so another
        // process cannot claim the suggested port. It is released immediately
        // before waking the Pin callback; the child cannot start before that
        // callback returns.
        let port_reservation = reserve_child_control_port();
        let control_port = port_reservation
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|address| address.port());
        let parent_control_port = super::super::RPC_PORT.load(core::sync::atomic::Ordering::Acquire);
        let mut handlers = with_registry(|registry| {
            let mut handlers = Vec::new();
            for (name, plugin) in registry {
                if plugin.state != STATE_RUNNING {
                    continue;
                }
                for (id, subscription) in &plugin.decisions {
                    if subscription.selector == DecisionSelector::ChildFollow {
                        handlers.push(Handler {
                            plugin: name.clone(),
                            id: *id,
                            callback: subscription.callback.clone_ref(py),
                            once: subscription.once,
                            order: subscription.order,
                        });
                    }
                }
            }
            handlers
        });
        sort_handlers(&mut handlers);

        let mut follow = !handlers.is_empty();
        let mut plugin_failed = false;
        for handler in handlers {
            let event = PyDict::new_bound(py);
            let argv = PyList::empty_bound(py);
            let argv_bytes = PyList::empty_bound(py);
            let mut build_error = None;
            for argument in &request.arguments {
                if let Err(error) = argv.append(String::from_utf8_lossy(argument).as_ref()) {
                    build_error = Some(error.to_string());
                    break;
                }
                if let Err(error) = argv_bytes.append(PyBytes::new_bound(py, argument)) {
                    build_error = Some(error.to_string());
                    break;
                }
            }
            if build_error.is_none() {
                let result: PyResult<()> = (|| {
                    event.set_item("type", "child.follow")?;
                    event.set_item("generation", request.generation)?;
                    event.set_item("process_id", request.process_id)?;
                    event.set_item("pid", request.process_id)?;
                    event.set_item("argv", argv)?;
                    event.set_item("argv_bytes", argv_bytes)?;
                    event.set_item("control_port", control_port)?;
                    event.set_item("parent_control_port", parent_control_port)?;
                    Ok(())
                })();
                if let Err(error) = result {
                    build_error = Some(error.to_string());
                }
            }

            let outcome = if let Some(error) = build_error {
                Err(format!("event build failed: {error}"))
            } else {
                with_plugin_context(&handler.plugin, || {
                    let _guard = PythonDecisionGuard::enter();
                    handler
                        .callback
                        .call1(py, (event,))
                        .map_err(|error| error.to_string())
                        .and_then(|value| parse_follow(value.bind(py)))
                })
            };
            match outcome {
                Ok(decision) => {
                    follow &= decision;
                    with_registry_mut(|registry| {
                        if let Some(plugin) = registry.get_mut(&handler.plugin) {
                            plugin.delivered = plugin.delivered.saturating_add(1);
                            if handler.once {
                                plugin.decisions.remove(&handler.id);
                            }
                        }
                    });
                }
                Err(error) => {
                    follow = false;
                    plugin_failed = true;
                    with_registry_mut(|registry| {
                        if let Some(plugin) = registry.get_mut(&handler.plugin) {
                            plugin.state = STATE_ERROR;
                            if handler.once {
                                plugin.decisions.remove(&handler.id);
                            }
                        }
                    });
                    output::push(
                        &handler.plugin,
                        &format!("pb.intercept(child.follow) failed: {error}"),
                    );
                    crate::log::line(&format!(
                        "plugin {} child.follow failed: {error}",
                        handler.plugin
                    ));
                }
            }
        }
        if plugin_failed {
            super::super::native_policies::refresh_best_effort("child interceptor failed");
        }
        if follow && (control_port.is_none() || parent_control_port == 0) {
            follow = false;
            crate::log::line("child.follow rejected: independent control port unavailable");
        }
        drop(port_reservation);
        crate::child_process::complete(
            request.generation,
            follow,
            control_port.unwrap_or(0),
            parent_control_port,
        );
    });
}
