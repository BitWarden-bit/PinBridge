use crate::{
    hub::{HubClient, HubError},
    tools,
};
use serde_json::{json, Map, Value};
use std::sync::Arc;

const SUPPORTED_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
const LATEST_PROTOCOL: &str = "2025-11-25";

pub struct Server {
    pub hub: Arc<dyn HubClient>,
}

impl Server {
    pub fn new(hub: Arc<dyn HubClient>) -> Self {
        Self { hub }
    }

    pub fn handle(&self, request: Value) -> Option<Value> {
        let object = match request.as_object() {
            Some(value) => value,
            None => {
                return Some(error_response(
                    Value::Null,
                    -32600,
                    "request must be an object",
                ))
            }
        };
        let id = object.get("id").cloned();
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error_response(
                id.unwrap_or(Value::Null),
                -32600,
                "jsonrpc must be \"2.0\"",
            ));
        }
        let method = match object.get("method").and_then(Value::as_str) {
            Some(value) => value,
            None => {
                return Some(error_response(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "missing method",
                ))
            }
        };
        let id = id?;
        match method {
            "initialize" => Some(initialize(id, object.get("params"))),
            "ping" => Some(json!({"jsonrpc":"2.0","id":id,"result":{}})),
            "tools/list" => {
                Some(json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools::definitions()}}))
            }
            "tools/call" => Some(self.call_tool(id, object.get("params"))),
            _ => Some(error_response(
                id,
                -32601,
                &format!("method not found: {method}"),
            )),
        }
    }

    fn call_tool(&self, id: Value, params: Option<&Value>) -> Value {
        let Some(params) = params.and_then(Value::as_object) else {
            return error_response(id, -32602, "tools/call params must be an object");
        };
        if params
            .keys()
            .any(|key| !matches!(key.as_str(), "name" | "arguments" | "_meta"))
        {
            return error_response(
                id,
                -32602,
                "tools/call params may contain only name, arguments, and _meta",
            );
        }
        if let Some(meta) = params.get("_meta") {
            if !meta.is_object() {
                return error_response(id, -32602, "tools/call _meta must be an object");
            }
        }
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error_response(id, -32602, "tools/call requires name");
        };
        if !tools::TOOL_NAMES.contains(&name) {
            return error_response(id, -32602, &format!("unknown tool: {name}"));
        }
        let arguments = match params.get("arguments") {
            None => Value::Object(Map::new()),
            Some(Value::Object(arguments)) => {
                if arguments.contains_key("actor") {
                    return error_response(
                        id,
                        -32602,
                        "actor is assigned by Hub and cannot be supplied",
                    );
                }
                Value::Object(arguments.clone())
            }
            Some(_) => return error_response(id, -32602, "tool arguments must be an object"),
        };
        match self.hub.call(name, &arguments) {
            Ok(result) => tool_ok_response(id, result),
            Err(HubError::Unavailable(message)) => {
                tool_error_response(id, None, &format!("session/control unavailable: {message}"))
            }
            Err(HubError::Execution {
                message,
                operation_id,
            }) => tool_error_response(id, operation_id.as_deref(), &message),
        }
    }
}

fn initialize(id: Value, params: Option<&Value>) -> Value {
    let Some(params) = params.and_then(Value::as_object) else {
        return error_response(id, -32602, "initialize params must be an object");
    };
    let Some(version) = params.get("protocolVersion").and_then(Value::as_str) else {
        return error_response(id, -32602, "initialize requires protocolVersion string");
    };
    if params
        .get("capabilities")
        .and_then(Value::as_object)
        .is_none()
    {
        return error_response(id, -32602, "initialize requires capabilities object");
    }
    let Some(client_info) = params.get("clientInfo").and_then(Value::as_object) else {
        return error_response(id, -32602, "initialize requires clientInfo object");
    };
    if client_info.get("name").and_then(Value::as_str).is_none()
        || client_info.get("version").and_then(Value::as_str).is_none()
    {
        return error_response(id, -32602, "clientInfo requires string name and version");
    }
    let response_version = if SUPPORTED_PROTOCOLS.contains(&version) {
        version
    } else {
        LATEST_PROTOCOL
    };
    json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":response_version,"capabilities":{"tools":{}},"serverInfo":{"name":"pinbridge-mcp","version":env!("CARGO_PKG_VERSION")}}})
}

fn tool_ok_response(id: Value, result: crate::hub::HubResult) -> Value {
    let mut structured = match result.value {
        Value::Object(map) => map,
        value => {
            let mut map = Map::new();
            map.insert("value".into(), value);
            map
        }
    };
    if let Some(operation_id) = result.operation_id {
        structured.insert("operation_id".into(), Value::String(operation_id));
    }
    let structured = Value::Object(structured);
    json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":serde_json::to_string(&structured).unwrap_or_else(|_| "{}".into())}],"structuredContent":structured}})
}

fn tool_error_response(id: Value, operation_id: Option<&str>, message: &str) -> Value {
    let mut error = Map::new();
    error.insert("message".into(), Value::String(message.into()));
    let mut structured = Map::new();
    structured.insert("error".into(), Value::Object(error));
    if let Some(operation_id) = operation_id {
        structured.insert("operation_id".into(), Value::String(operation_id.into()));
    }
    json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":message}],"structuredContent":Value::Object(structured)}})
}
fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        result: Result<crate::hub::HubResult, HubError>,
    }
    impl HubClient for Fake {
        fn call(&self, _: &str, _: &Value) -> Result<crate::hub::HubResult, HubError> {
            self.result.clone()
        }
    }
    fn request(method: &str, params: Value) -> Value {
        json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
    }
    #[test]
    fn standard_shapes_and_versions() {
        let server = Server::new(Arc::new(Fake {
            result: Ok(crate::hub::HubResult {
                value: json!({}),
                operation_id: None,
            }),
        }));
        let init = server.handle(request("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}))).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        let list = server.handle(request("tools/list", json!({}))).unwrap();
        assert!(list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "control_handoff_to_ai"
                && tool["name"] != "control_takeover_manual"));
    }

    #[test]
    fn rejects_non_jsonrpc_two_requests_but_ignores_valid_notifications() {
        let server = Server::new(Arc::new(Fake {
            result: Ok(crate::hub::HubResult {
                value: json!({}),
                operation_id: None,
            }),
        }));
        let mut wrong_version = request("ping", json!({}));
        wrong_version["jsonrpc"] = json!("1.0");
        assert_eq!(
            server.handle(wrong_version).unwrap()["error"]["code"],
            -32600
        );

        let mut notification = request("ping", json!({}));
        notification
            .as_object_mut()
            .expect("request object")
            .remove("id");
        assert!(server.handle(notification).is_none());

        let malformed = json!({"jsonrpc":"2.0","id":1});
        assert_eq!(server.handle(malformed).unwrap()["error"]["code"], -32600);
    }

    #[test]
    fn rejects_top_level_actor_and_unknown_call_fields() {
        let server = Server::new(Arc::new(Fake {
            result: Ok(crate::hub::HubResult {
                value: json!({}),
                operation_id: None,
            }),
        }));
        let response = server
            .handle(request(
                "tools/call",
                json!({"name":"control_status","actor":"human"}),
            ))
            .unwrap();
        assert_eq!(response["error"]["code"], -32602, "top-level actor");
        let response = server
            .handle(request(
                "tools/call",
                json!({"name":"control_status","unexpected":true}),
            ))
            .unwrap();
        assert_eq!(response["error"]["code"], -32602, "unknown field");
        let accepted_meta = server.handle(request(
            "tools/call",
            json!({"name":"control_status","_meta":{},"arguments":{}}),
        ));
        assert!(accepted_meta.unwrap()["result"].is_object());
    }

    #[test]
    fn tool_schemas_publish_string_safe_limits() {
        let tools = tools::definitions();
        let script = tools
            .iter()
            .find(|tool| tool["name"] == "script_inject")
            .expect("script_inject schema");
        assert_eq!(
            script["inputSchema"]["properties"]["name"]["maxLength"],
            256
        );
        assert_eq!(
            script["inputSchema"]["properties"]["source"]["maxLength"],
            1024 * 1024
        );
        assert_eq!(
            script["inputSchema"]["properties"]["source"]["type"],
            "string"
        );

        let resolve = tools
            .iter()
            .find(|tool| tool["name"] == "address_resolve")
            .expect("address_resolve schema");
        assert_eq!(
            resolve["inputSchema"]["properties"]["addresses"]["maxItems"],
            1024
        );
        assert_eq!(
            resolve["inputSchema"]["properties"]["addresses"]["items"]["type"],
            "string"
        );

        let activity = tools
            .iter()
            .find(|tool| tool["name"] == "activity_get")
            .expect("activity_get schema");
        assert_eq!(
            activity["inputSchema"]["properties"]["operation_id"]["pattern"],
            "^op-[0-9a-fA-F]{16}$"
        );
        assert_eq!(
            activity["inputSchema"]["properties"]["purpose"]["maxLength"],
            512
        );
    }
    #[test]
    fn forwarding_preserves_hub_operation_and_actor_cannot_be_forged() {
        let server = Server::new(Arc::new(Fake {
            result: Ok(crate::hub::HubResult {
                value: json!({"ok":true}),
                operation_id: Some("op-1".into()),
            }),
        }));
        let response = server
            .handle(request(
                "tools/call",
                json!({"name":"control_status","arguments":{}}),
            ))
            .unwrap();
        assert_eq!(
            response["result"]["structuredContent"]["operation_id"],
            "op-1"
        );
        let forged = server
            .handle(request(
                "tools/call",
                json!({"name":"control_status","arguments":{"actor":"human"}}),
            ))
            .unwrap();
        assert_eq!(forged["error"]["code"], -32602);
    }
    #[test]
    fn hub_error_is_call_tool_error_and_unavailable_is_clear() {
        let server = Server::new(Arc::new(Fake {
            result: Err(HubError::Execution {
                message: "denied".into(),
                operation_id: Some("op-2".into()),
            }),
        }));
        let response = server
            .handle(request(
                "tools/call",
                json!({"name":"memory_read","arguments":{}}),
            ))
            .unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert!(response.get("error").is_none());
        assert_eq!(
            response["result"]["structuredContent"]["operation_id"],
            "op-2"
        );
        let unavailable = Server::new(Arc::new(crate::hub::UnavailableHub {
            endpoint: "test".into(),
            credential_configured: false,
        }));
        let response = unavailable
            .handle(request(
                "tools/call",
                json!({"name":"session_status","arguments":{}}),
            ))
            .unwrap();
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unavailable"));
    }
}
