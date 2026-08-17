//! MCP tool catalogue and schemas. Execution is owned by Hub; this module
//! intentionally contains no Agent protocol or target lifecycle code.

use serde_json::{json, Map, Value};

pub const TOOL_NAMES: &[&str] = &[
    "control_status",
    "session_status",
    "target_pause",
    "target_resume",
    "target_step_into",
    "target_step_over",
    "breakpoint_set",
    "breakpoint_remove",
    "breakpoint_list",
    "registers_get",
    "register_set",
    "memory_read",
    "memory_write",
    "disassemble",
    "modules_list",
    "threads_list",
    "address_resolve",
    "script_inject",
    "script_replace",
    "script_remove",
    "script_list",
    "script_status",
    "script_output",
    "activity_list",
    "activity_get",
];

const MAX_SCRIPT_NAME_BYTES: usize = 256;
const MAX_SCRIPT_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_MEMORY_BYTES: usize = 1024 * 1024;
const MAX_RESOLVE_ADDRESSES: usize = 1024;
const MAX_OPERATION_ID_BYTES: usize = 19;
const MAX_PURPOSE_BYTES: usize = 512;

pub fn definitions() -> Vec<Value> {
    TOOL_NAMES.iter().map(|name| {
        let (description, properties, required) = match *name {
            "control_status" => ("Read Hub control/session status.", object_schema(vec![]), vec![]),
            "session_status" => ("Read the current Hub session and target status.", object_schema(vec![]), vec![]),
            "target_pause" => ("Pause the target through Hub policy.", object_schema(vec![]), vec![]),
            "target_resume" => ("Resume the target through Hub policy.", object_schema(vec![]), vec![]),
            "target_step_into" | "target_step_over" => ("Single-step a target thread through Hub.", object_schema(vec![("thread_id", integer_string_schema("hex or decimal thread id"))]), vec!["thread_id"]),
            "breakpoint_set" => ("Set a breakpoint through Hub.", object_schema(vec![("address", integer_string_schema("hex or decimal address"))]), vec!["address"]),
            "breakpoint_remove" => ("Remove a breakpoint through Hub.", object_schema(vec![("id", integer_string_schema("hex or decimal breakpoint id"))]), vec!["id"]),
            "breakpoint_list" => ("List breakpoints through Hub.", object_schema(vec![]), vec![]),
            "registers_get" => ("Read thread registers through Hub.", object_schema(vec![("thread_id", integer_string_schema("hex or decimal thread id"))]), vec!["thread_id"]),
            "register_set" => ("Set a register through Hub policy.", object_schema(vec![("thread_id", integer_string_schema("hex or decimal thread id")), ("register", bounded_string("register name or id", 64)), ("value", integer_string_schema("hex or decimal value"))]), vec!["thread_id", "register", "value"]),
            "memory_read" => ("Read bounded memory through Hub.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("size", decimal_string_schema("decimal byte count, at most 1 MiB", 20))]), vec!["address", "size"]),
            "memory_write" => ("Write bounded memory through Hub policy.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("data_hex", hex_bytes_schema("even-length hex, at most 1 MiB"))]), vec!["address", "data_hex"]),
            "disassemble" => ("Disassemble through Hub.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("count", decimal_string_schema("decimal instruction count, at most 4096", 4))]), vec!["address", "count"]),
            "modules_list" => ("List modules through Hub.", object_schema(vec![]), vec![]),
            "threads_list" => ("List threads through Hub.", object_schema(vec![]), vec![]),
            "address_resolve" => ("Resolve addresses through Hub.", object_schema(vec![("addresses", json!({"type":"array", "maxItems":MAX_RESOLVE_ADDRESSES, "items":integer_string_schema("hex or decimal address")})), ("name", bounded_string("module!export, at most 65535 bytes", 65535))]), vec![]),
            "script_inject" | "script_replace" => ("Load a bounded script through Hub.", object_schema(vec![("name", bounded_string("script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES)), ("source", bounded_string("script source, at most 1 MiB", MAX_SCRIPT_SOURCE_BYTES))]), vec!["name", "source"]),
            "script_remove" => ("Unload a script through Hub.", object_schema(vec![("name", bounded_string("script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES))]), vec!["name"]),
            "script_list" => ("List scripts through Hub.", object_schema(vec![]), vec![]),
            "script_status" => ("Read script status through Hub.", object_schema(vec![("name", bounded_string("optional script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES))]), vec![]),
            "script_output" => ("Read bounded script output through Hub.", object_schema(vec![("cursor", decimal_string_schema("decimal cursor, default 0", 20)), ("limit", output_limit_schema())]), vec![]),
            "activity_list" => ("Read Hub activity timeline.", object_schema(vec![("limit", decimal_string_schema("decimal limit, capped at 100", 20))]), vec![]),
            "activity_get" => ("Read one Hub activity by operation id.", object_schema(vec![("operation_id", operation_id_schema())]), vec!["operation_id"]),
            _ => unreachable!(),
        };
        let mut properties = properties;
        if let Value::Object(ref mut properties) = properties {
            properties.insert(
                "purpose".into(),
                bounded_string("AI-provided reason, at most 512 bytes", MAX_PURPOSE_BYTES),
            );
            properties.insert("parent_operation_id".into(), operation_id_schema());
        }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
    }).collect()
}

pub fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "target_pause"
            | "target_resume"
            | "target_step_into"
            | "target_step_over"
            | "breakpoint_set"
            | "breakpoint_remove"
            | "register_set"
            | "memory_write"
            | "script_inject"
            | "script_replace"
            | "script_remove"
    )
}

fn bounded_string(description: &str, max_length: usize) -> Value {
    json!({"type":"string","description":description,"maxLength":max_length})
}
fn integer_string_schema(description: &str) -> Value {
    json!({"type":"string","description":description,"maxLength":20,"pattern":"^(?:0[xX][0-9a-fA-F]+|[0-9]+)$"})
}
fn decimal_string_schema(description: &str, max_length: usize) -> Value {
    json!({"type":"string","description":description,"maxLength":max_length,"pattern":"^[0-9]+$"})
}
fn hex_bytes_schema(description: &str) -> Value {
    json!({"type":"string","description":description,"maxLength":MAX_MEMORY_BYTES * 2,"pattern":"^(?:[0-9a-fA-F]{2})*$"})
}
fn operation_id_schema() -> Value {
    json!({"type":"string","description":"Existing Hub operation id","maxLength":MAX_OPERATION_ID_BYTES,"pattern":"^op-[0-9a-fA-F]{16}$"})
}
fn output_limit_schema() -> Value {
    json!({"type":"string","description":"decimal limit, 1..1024","maxLength":4,"pattern":"^(?:[1-9]|[1-9][0-9]|[1-9][0-9]{2}|10[01][0-9]|102[0-4])$"})
}
fn object_schema(entries: Vec<(&str, Value)>) -> Value {
    let mut object = Map::new();
    for (key, value) in entries {
        object.insert(key.to_string(), value);
    }
    Value::Object(object)
}
