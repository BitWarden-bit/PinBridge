use crate::service::HubError;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_QUERY_EVENTS: u64 = 4096;
const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone)]
struct QuerySpec {
    limit: usize,
    after: u64,
    order: Order,
    layout: Layout,
    hook_types: Option<BTreeSet<String>>,
    phases: Option<BTreeSet<String>>,
    modules: Vec<String>,
    symbols: Vec<String>,
    thread_ids: Option<BTreeSet<u64>>,
    addresses: Option<BTreeSet<u64>>,
    group_by: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Asc,
    Desc,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Events,
    Calls,
    Summary,
}

pub(crate) fn requested_before(args: &Map<String, Value>) -> Result<u64, HubError> {
    parse_decimal(args, "before", 0, u64::MAX)
}

pub(crate) fn query(source: Value, args: &Map<String, Value>) -> Result<Value, HubError> {
    let spec = QuerySpec::parse(args)?;
    let lane = lane_metadata(&source);
    let source_events = source
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| HubError::Agent("Agent returned an invalid Hook event page".into()))?;
    let mut events = source_events
        .iter()
        .filter(|event| spec.matches(event))
        .cloned()
        .collect::<Vec<_>>();
    events.sort_by_key(event_sequence);
    let matched_events = events.len();

    let mut result = match spec.layout {
        Layout::Events => {
            if spec.order == Order::Desc {
                events.reverse();
            }
            events.truncate(spec.limit);
            json!({
                "layout": "events",
                "matched_events": matched_events.to_string(),
                "returned": events.len().to_string(),
                "events": events,
            })
        }
        Layout::Calls => {
            let mut calls = pair_calls(events);
            if spec.order == Order::Desc {
                calls.reverse();
            }
            let matched_calls = calls.len();
            calls.truncate(spec.limit);
            json!({
                "layout": "calls",
                "matched_events": matched_events.to_string(),
                "matched_calls": matched_calls.to_string(),
                "returned": calls.len().to_string(),
                "calls": calls,
            })
        }
        Layout::Summary => {
            let mut groups = summarize(&events, &spec.group_by);
            if spec.order == Order::Desc {
                groups.reverse();
            }
            let matched_groups = groups.len();
            groups.truncate(spec.limit);
            json!({
                "layout": "summary",
                "matched_events": matched_events.to_string(),
                "matched_groups": matched_groups.to_string(),
                "returned": groups.len().to_string(),
                "group_by": spec.group_by,
                "groups": groups,
            })
        }
    };
    if let (Some(result), Some(lane)) = (result.as_object_mut(), lane.as_object()) {
        result.insert("lane".into(), Value::Object(lane.clone()));
        result.insert("query".into(), spec.to_json());
    }
    Ok(result)
}

pub(crate) fn export(source: Value, args: &Map<String, Value>) -> Result<Value, HubError> {
    let queried = query(source, args)?;
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("jsonl");
    let (data, mime_type, extension) = match format {
        "json" => (
            serde_json::to_string_pretty(&queried)
                .map_err(|error| HubError::Internal(error.to_string()))?,
            "application/json",
            "json",
        ),
        "jsonl" => (export_jsonl(&queried)?, "application/x-ndjson", "jsonl"),
        "csv" => (
            export_csv(&queried, args)?,
            "text/csv; charset=utf-8",
            "csv",
        ),
        _ => {
            return Err(HubError::Validation(
                "Hook export format must be json, jsonl, or csv".into(),
            ))
        }
    };
    if data.len() > MAX_EXPORT_BYTES {
        return Err(HubError::Validation(format!(
            "Hook export exceeds the inline {} MiB limit; narrow the query",
            MAX_EXPORT_BYTES / (1024 * 1024)
        )));
    }
    let filename = args
        .get("filename")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("hook-events.{extension}"));
    Ok(json!({
        "delivery": "inline",
        "format": format,
        "mime_type": mime_type,
        "filename": filename,
        "bytes": data.len().to_string(),
        "data": data,
        "query_result": queried,
    }))
}

impl QuerySpec {
    fn parse(args: &Map<String, Value>) -> Result<Self, HubError> {
        let limit = parse_decimal(args, "limit", 1024, MAX_QUERY_EVENTS)? as usize;
        if limit == 0 {
            return Err(HubError::Validation(
                "Hook event query limit must be 1..4096".into(),
            ));
        }
        let after = parse_decimal(args, "after", 0, u64::MAX)?;
        let order = match args.get("order").and_then(Value::as_str).unwrap_or("desc") {
            "asc" => Order::Asc,
            "desc" => Order::Desc,
            _ => {
                return Err(HubError::Validation(
                    "Hook event order must be asc or desc".into(),
                ))
            }
        };
        let layout = match args
            .get("layout")
            .and_then(Value::as_str)
            .unwrap_or("events")
        {
            "events" => Layout::Events,
            "calls" => Layout::Calls,
            "summary" => Layout::Summary,
            _ => {
                return Err(HubError::Validation(
                    "Hook event layout must be events, calls, or summary".into(),
                ))
            }
        };
        let hook_types = enum_set(args, "hook_types", &["api", "instruction"])?;
        let phases = enum_set(args, "phases", &["hit", "entry", "return"])?;
        let modules = string_list(args, "modules", 64)?;
        let symbols = string_list(args, "symbols", 64)?;
        let thread_ids = numeric_set(args, "thread_ids")?;
        let addresses = numeric_set(args, "addresses")?;
        let group_by = {
            let values = string_list(args, "group_by", 8)?;
            let values = if values.is_empty() {
                vec!["display".to_string()]
            } else {
                values
            };
            for field in &values {
                if !matches!(
                    field.as_str(),
                    "module"
                        | "symbol"
                        | "display"
                        | "thread_id"
                        | "address"
                        | "hook_type"
                        | "kind"
                ) {
                    return Err(HubError::Validation(format!(
                        "unsupported Hook summary field: {field}"
                    )));
                }
            }
            values
        };
        Ok(Self {
            limit,
            after,
            order,
            layout,
            hook_types,
            phases,
            modules,
            symbols,
            thread_ids,
            addresses,
            group_by,
        })
    }

    fn matches(&self, event: &Value) -> bool {
        let sequence = event_sequence(event);
        if sequence <= self.after {
            return false;
        }
        if let Some(types) = &self.hook_types {
            if !event
                .get("hook_type")
                .and_then(Value::as_str)
                .is_some_and(|value| types.contains(value))
            {
                return false;
            }
        }
        if let Some(phases) = &self.phases {
            if !event
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|value| phases.contains(value))
            {
                return false;
            }
        }
        if !self.modules.is_empty()
            && !matches_patterns(event.get("module").and_then(Value::as_str), &self.modules)
        {
            return false;
        }
        if !self.symbols.is_empty()
            && !matches_patterns(event.get("symbol").and_then(Value::as_str), &self.symbols)
        {
            return false;
        }
        if let Some(thread_ids) = &self.thread_ids {
            if !event
                .get("thread_id")
                .and_then(parse_value_u64)
                .is_some_and(|value| thread_ids.contains(&value))
            {
                return false;
            }
        }
        if let Some(addresses) = &self.addresses {
            if !event
                .get("address")
                .and_then(parse_value_u64)
                .is_some_and(|value| addresses.contains(&value))
            {
                return false;
            }
        }
        true
    }

    fn to_json(&self) -> Value {
        json!({
            "limit": self.limit.to_string(),
            "after": self.after.to_string(),
            "order": if self.order == Order::Asc { "asc" } else { "desc" },
            "layout": match self.layout { Layout::Events => "events", Layout::Calls => "calls", Layout::Summary => "summary" },
            "hook_types": self.hook_types.as_ref().map(|values| values.iter().cloned().collect::<Vec<_>>()),
            "phases": self.phases.as_ref().map(|values| values.iter().cloned().collect::<Vec<_>>()),
            "modules": self.modules,
            "symbols": self.symbols,
            "thread_ids": self.thread_ids.as_ref().map(|values| values.iter().map(u64::to_string).collect::<Vec<_>>()),
            "addresses": self.addresses.as_ref().map(|values| values.iter().map(|value| format!("0x{value:x}")).collect::<Vec<_>>()),
            "group_by": self.group_by,
        })
    }
}

fn pair_calls(events: Vec<Value>) -> Vec<Value> {
    let mut stacks: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    let mut calls = Vec::new();
    for event in events {
        let kind = event.get("kind").and_then(Value::as_str).unwrap_or("hit");
        if kind == "hit" {
            calls.push(call_from_parts(Some(event), None, "hit"));
            continue;
        }
        let key = (
            scalar_string(event.get("thread_id")),
            scalar_string(event.get("address")),
        );
        if kind == "entry" {
            stacks.entry(key).or_default().push(event);
        } else if let Some(entry) = stacks.get_mut(&key).and_then(Vec::pop) {
            calls.push(call_from_parts(Some(entry), Some(event), "paired"));
        } else {
            calls.push(call_from_parts(None, Some(event), "unmatched_return"));
        }
    }
    for entries in stacks.into_values() {
        for entry in entries {
            calls.push(call_from_parts(Some(entry), None, "unmatched_entry"));
        }
    }
    calls.sort_by_key(event_sequence);
    calls
}

fn call_from_parts(entry: Option<Value>, ret: Option<Value>, status: &str) -> Value {
    let primary = entry.as_ref().or(ret.as_ref()).expect("call has an event");
    let entry_sequence = entry.as_ref().map(event_sequence);
    let return_sequence = ret.as_ref().map(event_sequence);
    let entry_time = entry
        .as_ref()
        .and_then(|value| value.get("timestamp_unix_ns"))
        .and_then(parse_value_u64);
    let return_time = ret
        .as_ref()
        .and_then(|value| value.get("timestamp_unix_ns"))
        .and_then(parse_value_u64);
    json!({
        "sequence": entry_sequence.or(return_sequence).unwrap_or(0).to_string(),
        "status": status,
        "thread_id": primary.get("thread_id").cloned(),
        "address": primary.get("address").cloned(),
        "module": primary.get("module").cloned(),
        "symbol": primary.get("symbol").cloned(),
        "display": primary.get("display").cloned(),
        "hook_type": primary.get("hook_type").cloned(),
        "entry_sequence": entry_sequence.map(|value| value.to_string()),
        "return_sequence": return_sequence.map(|value| value.to_string()),
        "duration_ns": entry_time.zip(return_time).map(|(start, end)| end.saturating_sub(start).to_string()),
        "arguments": entry.as_ref().and_then(|value| value.get("arguments")).cloned(),
        "typed_arguments": entry.as_ref().and_then(|value| value.get("typed_arguments")).cloned(),
        "return_value": ret.as_ref().and_then(|value| value.get("return_value")).cloned(),
        "typed_return": ret.as_ref().and_then(|value| value.get("typed_return")).cloned(),
        "entry": entry,
        "return": ret,
    })
}

fn summarize(events: &[Value], group_by: &[String]) -> Vec<Value> {
    #[derive(Default)]
    struct Group {
        key: Map<String, Value>,
        count: u64,
        first_sequence: u64,
        last_sequence: u64,
        first_timestamp: u64,
        last_timestamp: u64,
    }
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for event in events {
        let mut key = Map::new();
        for field in group_by {
            key.insert(
                field.clone(),
                event.get(field).cloned().unwrap_or(Value::Null),
            );
        }
        let encoded = serde_json::to_string(&key).unwrap_or_default();
        let sequence = event_sequence(event);
        let timestamp = event
            .get("timestamp_unix_ns")
            .and_then(parse_value_u64)
            .unwrap_or(0);
        let group = groups.entry(encoded).or_insert_with(|| Group {
            key,
            first_sequence: sequence,
            last_sequence: sequence,
            first_timestamp: timestamp,
            last_timestamp: timestamp,
            ..Group::default()
        });
        group.count += 1;
        group.first_sequence = group.first_sequence.min(sequence);
        group.last_sequence = group.last_sequence.max(sequence);
        group.first_timestamp = group.first_timestamp.min(timestamp);
        group.last_timestamp = group.last_timestamp.max(timestamp);
    }
    let mut values = groups
        .into_values()
        .map(|group| {
            json!({
                "key": group.key,
                "count": group.count.to_string(),
                "first_sequence": group.first_sequence.to_string(),
                "last_sequence": group.last_sequence.to_string(),
                "first_timestamp_unix_ns": group.first_timestamp.to_string(),
                "last_timestamp_unix_ns": group.last_timestamp.to_string(),
            })
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.get("count").and_then(parse_value_u64).unwrap_or(0));
    values
}

fn export_jsonl(result: &Value) -> Result<String, HubError> {
    let rows = result_rows(result)?;
    let mut data = String::new();
    for row in rows {
        data.push_str(
            &serde_json::to_string(row).map_err(|error| HubError::Internal(error.to_string()))?,
        );
        data.push('\n');
    }
    Ok(data)
}

fn export_csv(result: &Value, args: &Map<String, Value>) -> Result<String, HubError> {
    let rows = result_rows(result)?;
    let layout = result
        .get("layout")
        .and_then(Value::as_str)
        .unwrap_or("events");
    let fields = {
        let requested = string_list(args, "fields", 64)?;
        if !requested.is_empty() {
            requested
        } else {
            match layout {
                "calls" => vec![
                    "sequence",
                    "status",
                    "thread_id",
                    "address",
                    "module",
                    "symbol",
                    "display",
                    "duration_ns",
                    "arguments",
                    "return_value",
                ],
                "summary" => vec![
                    "key",
                    "count",
                    "first_sequence",
                    "last_sequence",
                    "first_timestamp_unix_ns",
                    "last_timestamp_unix_ns",
                ],
                _ => vec![
                    "sequence",
                    "timestamp_unix_ns",
                    "kind",
                    "hook_type",
                    "thread_id",
                    "address",
                    "module",
                    "symbol",
                    "display",
                    "arguments",
                    "return_value",
                ],
            }
            .into_iter()
            .map(str::to_string)
            .collect()
        }
    };
    let mut data = String::new();
    data.push_str(
        &fields
            .iter()
            .map(|field| csv_cell(field))
            .collect::<Vec<_>>()
            .join(","),
    );
    data.push_str("\r\n");
    for row in rows {
        let line = fields
            .iter()
            .map(|field| csv_cell(&scalar_string(row.get(field))))
            .collect::<Vec<_>>()
            .join(",");
        data.push_str(&line);
        data.push_str("\r\n");
    }
    Ok(data)
}

fn result_rows(result: &Value) -> Result<&Vec<Value>, HubError> {
    for key in ["events", "calls", "groups"] {
        if let Some(rows) = result.get(key).and_then(Value::as_array) {
            return Ok(rows);
        }
    }
    Err(HubError::Internal(
        "Hook query result has no row collection".into(),
    ))
}

fn lane_metadata(source: &Value) -> Value {
    let mut lane = Map::new();
    for field in [
        "lane_total",
        "lane_dropped",
        "history_overwritten",
        "next_cursor",
        "capacity",
        "pointer_width",
        "window_before",
    ] {
        if let Some(value) = source.get(field) {
            lane.insert(field.to_string(), value.clone());
        }
    }
    Value::Object(lane)
}

fn event_sequence(value: &Value) -> u64 {
    value.get("sequence").and_then(parse_value_u64).unwrap_or(0)
}

fn parse_value_u64(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .map(|hex| u64::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| text.parse().ok())
}

fn parse_decimal(
    args: &Map<String, Value>,
    key: &str,
    default: u64,
    max: u64,
) -> Result<u64, HubError> {
    let value = match args.get(key) {
        None => default,
        Some(Value::String(value)) => value
            .parse::<u64>()
            .map_err(|_| HubError::Validation(format!("{key} must be decimal")))?,
        Some(_) => {
            return Err(HubError::Validation(format!(
                "{key} must be a decimal string"
            )))
        }
    };
    if value > max {
        return Err(HubError::Validation(format!("{key} exceeds {max}")));
    }
    Ok(value)
}

fn string_list(args: &Map<String, Value>, key: &str, max: usize) -> Result<Vec<String>, HubError> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| HubError::Validation(format!("{key} must be an array")))?;
    if values.len() > max {
        return Err(HubError::Validation(format!(
            "{key} accepts at most {max} values"
        )));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| HubError::Validation(format!("{key} values must be strings")))
        })
        .collect()
}

fn enum_set(
    args: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<Option<BTreeSet<String>>, HubError> {
    let values = string_list(args, key, allowed.len())?;
    if values.is_empty() {
        return Ok(None);
    }
    let mut result = BTreeSet::new();
    for value in values {
        if !allowed.contains(&value.as_str()) {
            return Err(HubError::Validation(format!(
                "unsupported {key} value: {value}"
            )));
        }
        result.insert(value);
    }
    Ok(Some(result))
}

fn numeric_set(args: &Map<String, Value>, key: &str) -> Result<Option<BTreeSet<u64>>, HubError> {
    let values = string_list(args, key, 1024)?;
    if values.is_empty() {
        return Ok(None);
    }
    values
        .into_iter()
        .map(|value| {
            value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
                .map(|hex| u64::from_str_radix(hex, 16))
                .unwrap_or_else(|| value.parse())
                .map_err(|_| {
                    HubError::Validation(format!("invalid numeric value in {key}: {value}"))
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(Some)
}

fn scalar_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(value)) => value.clone(),
        Some(value) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn matches_patterns(value: Option<&str>, patterns: &[String]) -> bool {
    value.is_some_and(|value| {
        patterns
            .iter()
            .any(|pattern| wildcard_match(pattern, value))
    })
}

pub(crate) fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase().into_bytes();
    let value = value.to_ascii_lowercase().into_bytes();
    let (mut p, mut v, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = v;
        } else if let Some(star_at) = star {
            p = star_at + 1;
            retry += 1;
            v = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_is_case_insensitive() {
        assert!(wildcard_match("Nt*File", "NtCreateFile"));
        assert!(!wildcard_match("Zw?pen", "ZwCreateFile"));
    }

    #[test]
    fn pairs_nested_calls_by_thread_and_address() {
        let events = vec![
            json!({"sequence":"1","kind":"entry","thread_id":"7","address":"0x10"}),
            json!({"sequence":"2","kind":"entry","thread_id":"7","address":"0x10"}),
            json!({"sequence":"3","kind":"return","thread_id":"7","address":"0x10"}),
            json!({"sequence":"4","kind":"return","thread_id":"7","address":"0x10"}),
        ];
        let calls = pair_calls(events);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["entry_sequence"], "1");
        assert_eq!(calls[0]["return_sequence"], "4");
        assert_eq!(calls[1]["entry_sequence"], "2");
        assert_eq!(calls[1]["return_sequence"], "3");
    }
}
