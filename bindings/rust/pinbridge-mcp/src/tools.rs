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
    "breakpoint_inventory",
    "registers_get",
    "register_set",
    "memory_read",
    "memory_write",
    "disassemble",
    "modules_list",
    "module_exports",
    "hook_targets_query",
    "hook_monitor_apply",
    "hook_set",
    "hook_function_set",
    "hook_signature_set",
    "hook_signature_remove",
    "hook_remove",
    "hook_clear",
    "hook_list",
    "hook_inventory",
    "event_index_query",
    "event_index_export",
    "trace_scope_query",
    "trace_record_start",
    "trace_record_status",
    "trace_record_stop",
    "trace_index_query",
    "trace_index_export",
    "threads_list",
    "address_resolve",
    "exception_monitor",
    "exception_policy_get",
    "exception_policy_set",
    "exception_inventory",
    "script_inject",
    "script_replace",
    "script_start",
    "script_stop",
    "script_remove",
    "script_list",
    "script_get",
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
            "breakpoint_inventory" => ("List breakpoints with traditional ownership, callback bindings, and each callback's latest bounded full return value, parsed control action, or error.", object_schema(vec![]), vec![]),
            "registers_get" => ("Read thread registers through Hub.", object_schema(vec![("thread_id", integer_string_schema("hex or decimal thread id"))]), vec!["thread_id"]),
            "register_set" => ("Set a register through Hub policy.", object_schema(vec![("thread_id", integer_string_schema("hex or decimal thread id")), ("register", bounded_string("register name or id", 64)), ("value", integer_string_schema("hex or decimal value"))]), vec!["thread_id", "register", "value"]),
            "memory_read" => ("Read bounded memory through Hub.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("size", decimal_string_schema("decimal byte count, at most 1 MiB", 20))]), vec!["address", "size"]),
            "memory_write" => ("Write bounded memory through Hub policy.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("data_hex", hex_bytes_schema("even-length hex, at most 1 MiB"))]), vec!["address", "data_hex"]),
            "disassemble" => ("Disassemble through Hub.", object_schema(vec![("address", integer_string_schema("hex or decimal address")), ("count", decimal_string_schema("decimal instruction count, at most 4096", 4))]), vec!["address", "count"]),
            "modules_list" => ("List modules through Hub.", object_schema(vec![]), vec![]),
            "module_exports" => ("List real named exports and addresses for one loaded module.", object_schema(vec![("module", bounded_string("loaded module name or path", 1024))]), vec!["module"]),
            "hook_targets_query" => ("Query an immutable, bounded snapshot of named module exports without changing the target. The result includes a selection id, exact count, and deterministic digest that must be confirmed by hook_monitor_apply.", object_schema(vec![("module", bounded_string("loaded module name or path", 1024)), ("symbol_pattern", bounded_string("case-insensitive wildcard over export names; * and ? are supported; default *", 512)), ("offset", decimal_string_schema("zero-based target preview offset; default 0", 5)), ("limit", decimal_string_schema("target preview size 1..4096; default 256", 4))]), vec!["module"]),
            "hook_monitor_apply" => ("Explicitly apply one previously queried Hook target snapshot in native monitor mode. selection_id, expected_count, and selection_digest must all match, preventing stale or implicit bulk mutation.", object_schema(vec![("selection_id", hook_selection_id_schema()), ("expected_count", decimal_string_schema("exact selected target count returned by hook_targets_query", 5)), ("selection_digest", hook_selection_digest_schema()), ("mode", json!({"type":"string","enum":["monitor"],"description":"native asynchronous monitoring; callbacks are configured separately"}))]), vec!["selection_id", "expected_count", "selection_digest"]),
            "hook_set" => ("Install one native instruction Hook. Hits can be observed or synchronously intercepted from Python without using breakpoint slots.", object_schema(vec![("address", integer_string_schema("hex or decimal instruction address"))]), vec!["address"]),
            "hook_function_set" => ("Install one signature-driven function-call Hook. The C-like signature and its provenance are mandatory; Agent uses it to capture integer/XMM/stack arguments and the declared return location instead of guessing from raw registers.", object_schema(vec![("address", integer_string_schema("hex or decimal function entry address")), ("signature", bounded_string("C-like prototype, for example: int DemoApi(int value)", 2048)), ("signature_source", signature_source_schema()), ("signature_confidence", confidence_schema())]), vec!["address", "signature", "signature_source", "signature_confidence"]),
            "hook_signature_set" => ("Install or replace typed capture metadata for one function address without changing Hook ownership. Use pdb/header for authoritative prototypes, manual for operator declarations, or ai_inferred for an explicitly uncertain reverse-engineered prototype.", object_schema(vec![("address", integer_string_schema("hex or decimal function entry address")), ("signature", bounded_string("C-like function prototype", 2048)), ("signature_source", signature_source_schema()), ("signature_confidence", confidence_schema())]), vec!["address", "signature", "signature_source", "signature_confidence"]),
            "hook_signature_remove" => ("Remove typed capture metadata for one function. The physical Hook remains and future events explicitly fall back to raw ABI slots.", object_schema(vec![("address", integer_string_schema("hex or decimal function entry address"))]), vec!["address"]),
            "hook_remove" => ("Remove one native instruction Hook. Active synchronous callback bindings must be removed through their owning script first.", object_schema(vec![("address", integer_string_schema("hex or decimal instruction address"))]), vec!["address"]),
            "hook_clear" => ("Clear native Hooks only when no synchronous Hook callbacks are active.", object_schema(vec![]), vec![]),
            "hook_list" => ("List native Hook instruction addresses.", object_schema(vec![]), vec![]),
            "hook_inventory" => ("Page Hooks with strict API/function versus plain-instruction separation, symbol resolution, signature provenance/confidence, ownership, callbacks, and latest callback result.", object_schema(vec![("offset", decimal_string_schema("zero-based Hook offset, 0..32768; default 0", 5)), ("limit", decimal_string_schema("page size, 1..4096; default 1000", 4)), ("kind", json!({"type":"string", "enum":["all", "api", "instruction"], "description":"optional strict Hook type filter"}))]), vec![]),
            "hook_monitor" => ("Read the dedicated time-ordered Hook log. API/function Hooks and plain instruction Hooks are explicitly distinguished. Signature-backed API entries contain typed arguments and paired returns; missing signatures remain raw ABI. Use before with the first sequence of the current page to browse older retained history.", object_schema(vec![("limit", decimal_string_schema("Hook events per page, 1..4096; default 1024", 4)), ("before", decimal_string_schema("optional exclusive sequence cursor; omit for newest", 20))]), vec![]),
            "event_index_query" => ("Query the retained event indices using an AI-selected exact key. Examples: index=syscall key=50, or index=api module=ntdll.dll key=NtCreateFile. limit is mandatory so the caller explicitly controls context cost; payload defaults false and fields can project only needed columns.", object_schema(event_index_properties(false)), vec!["index", "key", "limit"]),
            "event_index_export" => ("Export the result of one AI-selected event index query. delivery=file is the default and returns only compact file metadata, avoiding event data in MCP context; delivery=inline must be explicitly requested.", object_schema(event_index_properties(true)), vec!["index", "key", "limit"]),
            "trace_scope_query" => ("Resolve and freeze one immutable Trace recording scope without changing the target. Omit ranges to select the whole loaded module, or provide up to 16 module-relative half-open RVA ranges. The returned id, exact normalized range count, and digest must be confirmed by trace_record_start.", object_schema(vec![("module", bounded_string("loaded module name or path", 1024)), ("kinds", trace_kinds_schema()), ("ranges", trace_ranges_schema()), ("threads", json!({"type":"array","maxItems":64,"uniqueItems":true,"description":"optional target thread ids; omit or empty means all threads","items":integer_string_schema("hex or decimal target thread id")}))]), vec!["module", "kinds"]),
            "trace_record_start" => ("Explicitly start one native .pbtr recording from a previously queried immutable scope. All confirmation fields must match; Hub chooses a controlled output directory and accepts only a safe basename.", object_schema(vec![("selection_id", trace_selection_id_schema()), ("expected_count", decimal_string_schema("exact normalized range count returned by trace_scope_query", 2)), ("selection_digest", trace_selection_digest_schema()), ("filename", bounded_string("safe basename; .pbtr is added if omitted", 133))]), vec!["selection_id", "expected_count", "selection_digest"]),
            "trace_record_status" => ("Read recorder state, counters, frozen scope, and current artifact metadata without returning trace payload.", object_schema(vec![]), vec![]),
            "trace_record_stop" => ("Stop the current native Trace, finish the local .pbtr file, and build its rebuildable SQLite query database. No event rows are returned.", object_schema(vec![]), vec![]),
            "trace_index_query" => ("Query the completed current-session Trace from its local SQLite database without contacting the Agent. limit is mandatory and capped at 256; payload and metadata default false, and fields can request a smaller projection. Raw path input is intentionally not accepted.", object_schema(trace_index_properties(false)), vec!["index", "key", "limit"]),
            "trace_index_export" => ("Export one bounded result from the completed Trace's local SQLite database without contacting the Agent. delivery=file is the default and returns only compact artifact metadata; inline must be explicit.", object_schema(trace_index_properties(true)), vec!["index", "key", "limit"]),
            "hook_module" => ("Enable function-call logging for every unique named export of one loaded DLL in one batch. Addresses with registered signatures are captured and decoded by type; exports without signatures remain explicitly raw ABI. Aliases are deduplicated.", object_schema(vec![("module", bounded_string("loaded DLL name or path", 1024))]), vec!["module"]),
            "threads_list" => ("List threads through Hub.", object_schema(vec![]), vec![]),
            "address_resolve" => ("Resolve addresses through Hub.", object_schema(vec![("addresses", json!({"type":"array", "maxItems":MAX_RESOLVE_ADDRESSES, "items":integer_string_schema("hex or decimal address")})), ("name", bounded_string("module!export, at most 65535 bytes", 65535))]), vec![]),
            "exception_monitor" => ("Read retained target and Pin-internal exceptions from the dedicated high-priority lane.", object_schema(vec![("limit", decimal_string_schema("decimal limit, 1..1024; default 256", 4))]), vec![]),
            "exception_policy_get" => ("Read the target-exception pause policy and pending state.", object_schema(vec![]), vec![]),
            "exception_policy_set" => ("Enable or disable exception-triggered pause; code 0 matches every target exception.", object_schema(vec![("enabled", json!({"type":"boolean"})), ("code", integer_string_schema("hex or decimal native exception code; 0 means all"))]), vec!["enabled", "code"]),
            "exception_inventory" => ("List live exception.handle interceptors, filters, callback identity, provenance, and latest bounded return or error.", object_schema(vec![]), vec![]),
            "script_inject" | "script_replace" => ("Load or update a bounded script through Hub. kind=module is an independent, stateful analysis module; kind=callback is owned by one focused callback workflow. Every pb.breakpoint and pb.intercept callback must declare a non-empty keyword-only description= literal (maximum 512 bytes) explaining its purpose, filter, and permitted changes.", object_schema(vec![("name", bounded_string("script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES)), ("source", bounded_string("script source, at most 1 MiB; every pb.breakpoint/pb.intercept call requires literal description=", MAX_SCRIPT_SOURCE_BYTES)), ("kind", json!({"type":"string","enum":["module","callback"],"description":"resource kind; defaults to module"}))]), vec!["name", "source"]),
            "script_start" => ("Start a stopped module script from the source retained by the current Hub.", object_schema(vec![("name", bounded_string("module script name", MAX_SCRIPT_NAME_BYTES))]), vec!["name"]),
            "script_stop" => ("Stop a module script and release its runtime-owned callbacks and policies while retaining source for restart.", object_schema(vec![("name", bounded_string("module script name", MAX_SCRIPT_NAME_BYTES))]), vec!["name"]),
            "script_remove" => ("Unload and delete a script resource through Hub.", object_schema(vec![("name", bounded_string("script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES))]), vec!["name"]),
            "script_list" => ("List module and callback script resources through Hub, including their explicit kind.", object_schema(vec![]), vec![]),
            "script_get" => ("Read source and provenance for a script loaded through the current Hub.", object_schema(vec![("name", bounded_string("script name, at most 256 bytes", MAX_SCRIPT_NAME_BYTES))]), vec!["name"]),
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
            | "hook_set"
            | "hook_function_set"
            | "hook_signature_set"
            | "hook_signature_remove"
            | "hook_remove"
            | "hook_clear"
            | "hook_monitor_apply"
            | "trace_record_start"
            | "trace_record_stop"
            | "hook_module"
            | "register_set"
            | "memory_write"
            | "script_inject"
            | "script_replace"
            | "script_start"
            | "script_stop"
            | "script_remove"
            | "exception_policy_set"
    )
}

fn bounded_string(description: &str, max_length: usize) -> Value {
    json!({"type":"string","description":description,"maxLength":max_length})
}
fn signature_source_schema() -> Value {
    json!({"type":"string","description":"Where the prototype came from; ai_inferred is never presented as authoritative","enum":["pdb","header","manual","ai_inferred"]})
}
fn confidence_schema() -> Value {
    json!({"type":"string","description":"decimal confidence 0..100","maxLength":3,"pattern":"^(?:[0-9]|[1-9][0-9]|100)$"})
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
fn hook_selection_id_schema() -> Value {
    json!({"type":"string","description":"immutable Hook target snapshot id","maxLength":24,"pattern":"^hooksel-[0-9a-fA-F]{16}$"})
}
fn hook_selection_digest_schema() -> Value {
    json!({"type":"string","description":"deterministic digest returned by hook_targets_query","maxLength":24,"pattern":"^fnv1a64:[0-9a-fA-F]{16}$"})
}
fn trace_selection_id_schema() -> Value {
    json!({"type":"string","description":"immutable Trace scope snapshot id","maxLength":25,"pattern":"^tracesel-[0-9a-fA-F]{16}$"})
}
fn trace_selection_digest_schema() -> Value {
    json!({"type":"string","description":"deterministic digest returned by trace_scope_query","maxLength":24,"pattern":"^fnv1a64:[0-9a-fA-F]{16}$"})
}
fn trace_kinds_schema() -> Value {
    json!({
        "type":"array",
        "minItems":1,
        "maxItems":8,
        "uniqueItems":true,
        "description":"exec and memory include bytes/values; *_plain records smaller legacy rows",
        "items":{"type":"string","enum":["exec","memory","branch","syscall","exception","registers","exec_plain","memory_plain"]}
    })
}
fn trace_ranges_schema() -> Value {
    json!({
        "type":"array",
        "maxItems":16,
        "description":"optional half-open module-relative ranges; omitted or empty selects the whole module",
        "items":{
            "type":"object",
            "properties":{
                "rva_begin":integer_string_schema("inclusive module-relative begin"),
                "rva_end":integer_string_schema("exclusive module-relative end")
            },
            "required":["rva_begin","rva_end"],
            "additionalProperties":false
        }
    })
}
fn trace_index_properties(export: bool) -> Vec<(&'static str, Value)> {
    let mut properties = vec![
        (
            "index",
            json!({"type":"string","enum":["kind","address","thread","sequence","memory"],"description":"exact on-disk Trace index selected by the AI"}),
        ),
        (
            "key",
            bounded_string(
                "kind name/id, instruction address, thread id, sequence, or memory address",
                64,
            ),
        ),
        (
            "limit",
            decimal_string_schema(
                "mandatory maximum matching rows, 1..256; choose the smallest useful value",
                3,
            ),
        ),
        (
            "before",
            decimal_string_schema("optional exclusive sequence cursor from next_before", 20),
        ),
        (
            "payload",
            json!({"type":"boolean","description":"include kind-specific data such as bytes, memory values, branch targets, syscall arguments, and registers; default false"}),
        ),
        (
            "metadata",
            json!({"type":"boolean","description":"include PBTR JSON metadata; default false"}),
        ),
        (
            "fields",
            json!({"type":"array","maxItems":32,"uniqueItems":true,"items":{"type":"string","enum":["sequence","kind","kind_id","thread_id","address","size","bytes","memory","access","value","target","taken","number","phase","arguments","return_value","errno","reason","info","context_ip","tag","marker_value","frame","reg_id","width","part","repeat_count","original_kind","args"]}}),
        ),
    ];
    if export {
        properties.extend([
            (
                "format",
                json!({"type":"string","enum":["json","jsonl","csv"],"description":"default jsonl"}),
            ),
            (
                "delivery",
                json!({"type":"string","enum":["file","inline"],"description":"default file; inline is capped at 2 MiB and consumes MCP context"}),
            ),
            (
                "filename",
                bounded_string("safe basename without a path", 128),
            ),
        ]);
    }
    properties
}
fn event_index_properties(export: bool) -> Vec<(&'static str, Value)> {
    let mut properties = vec![
        (
            "index",
            json!({"type":"string","enum":["api","syscall","address","thread"],"description":"server-side event index chosen by the AI"}),
        ),
        (
            "key",
            bounded_string(
                "exact key or wildcard: API symbol, syscall number, address, or thread id",
                512,
            ),
        ),
        (
            "module",
            bounded_string(
                "required API module name/wildcard, for example ntdll.dll",
                1024,
            ),
        ),
        (
            "source",
            json!({"type":"string","enum":["hook","syscall"],"description":"lane for a thread index; default hook"}),
        ),
        (
            "limit",
            decimal_string_schema(
                "mandatory maximum matching rows, 1..256; choose the smallest useful value",
                3,
            ),
        ),
        (
            "before",
            decimal_string_schema("optional exclusive sequence cursor from next_before", 20),
        ),
        (
            "phases",
            json!({"type":"array","maxItems":4,"uniqueItems":true,"items":{"type":"string","enum":["hit","entry","return","exit"]}}),
        ),
        (
            "payload",
            json!({"type":"boolean","description":"include arguments, typed values, return value, errno, and signature; default false"}),
        ),
        (
            "fields",
            json!({"type":"array","maxItems":32,"uniqueItems":true,"items":{"type":"string","enum":["sequence","timestamp_unix_ns","generation","kind","phase","hook_type","thread_id","address","module","symbol","display","signature_capture","signature_status","capture_status","argument_count","arguments","typed_arguments","return_value","typed_return","errno","number","number_decimal"]}}),
        ),
    ];
    if export {
        properties.extend([
            ("format", json!({"type":"string","enum":["json","jsonl","csv"],"description":"default jsonl"})),
            ("delivery", json!({"type":"string","enum":["file","inline"],"description":"default file; inline is capped at 2 MiB and consumes MCP context"})),
            ("filename", bounded_string("safe basename without a path; extension is selected by format", 128)),
        ]);
    }
    properties
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
