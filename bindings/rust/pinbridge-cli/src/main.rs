//! pinbridge-cli: headless console for pinbridge-agent.
//!
//! The UI (Tauri/TUI) is a cosmetic layer only: every capability lands here
//! first as a scriptable command with JSON output, so the whole system is
//! usable and testable without a display.
//!
//! Usage:
//!   pinbridge-cli [--port N] ping
//!   pinbridge-cli [--port N] counters
//!   pinbridge-cli [--port N] events [--limit N]
//!   pinbridge-cli [backend options] run <pin.exe> -- <target> [args...]
//!
//! With `run`, the backend is spawned first and reaped after the command
//! finishes. All output is JSON on stdout; errors go to stderr, exit != 0.

use pinbridge_client::{client::Client, launch, registers, Arch};
use pinbridge_proto as proto;
use std::collections::HashSet;
use std::time::Duration;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

struct Config {
    port: u16,
    command: String,
    args: Vec<String>,
    limit: u64,
    target: Vec<String>,
    pin: Option<String>,
    agent: Option<String>,
    /// Explicit --arch override; None means auto-detect from the target's PE.
    arch: Option<Arch>,
    entry_bp: bool,
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        port: proto::DEFAULT_PORT,
        command: String::new(),
        args: Vec::new(),
        limit: 16,
        target: Vec::new(),
        pin: None,
        agent: None,
        arch: None,
        entry_bp: true,
    };
    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                let value = args.next().ok_or("--port needs a value")?;
                config.port = value.parse().map_err(|_| "bad --port")?;
            }
            "--limit" => {
                let value = args.next().ok_or("--limit needs a value")?;
                config.limit = value.parse().map_err(|_| "bad --limit")?;
            }
            "--pin" => config.pin = Some(args.next().ok_or("--pin needs a value")?),
            "--agent" => config.agent = Some(args.next().ok_or("--agent needs a value")?),
            "--arch" => {
                let value = args.next().ok_or("--arch needs a value")?;
                // `auto` means "read the target's PE headers"; x86/x64 pin it.
                config.arch = if value.eq_ignore_ascii_case("auto") {
                    None
                } else {
                    Some(Arch::parse(&value)?)
                };
            }
            "--entry-bp" => config.entry_bp = true,
            "--no-entry-bp" => config.entry_bp = false,
            // subcommand flags that belong to the command, not the top level
            "--follow" => config.args.push(arg.clone()),
            "--" => config.target.extend(args.by_ref()),
            other if other.starts_with("--") => return Err(format!("unknown option: {other}")),
            other if config.command.is_empty() => config.command = other.to_string(),
            other => config.args.push(other.to_string()),
        }
    }
    Ok(config)
}

fn usage() {
    eprintln!(
        "pinbridge-cli [--port N] [--limit N] [--pin P] [--agent A] [--arch auto|x86|x64] [--entry-bp|--no-entry-bp] [command] [-- target args...]\n\
         commands: ping | counters | events | hookrule | shell (default when omitted)\n\
         a target after `--` is launched first (backend reaped on exit).\n\
         shell reads one command per line from stdin: ping|counters|events [N]|limit N|help|quit"
    );
}

fn event_json(event: &proto::EventRecord) -> serde_json::Value {
    serde_json::json!({
        "sequence": event.sequence,
        "kind": event.kind,
        "thread_id": event.thread_id,
        "address": event.address,
        "arg0": event.arg0,
        "arg1": event.arg1,
        "arg2": event.arg2,
        "arg3": event.arg3,
        "arg4": event.arg4,
        "arg5": event.arg5,
        "arg6": event.arg6,
        "arg7": event.arg7,
        "kind_name": match event.kind {
            14 => "hook_return",
            _ => match event.kind {
                1 => "hook_regs", 2 => "memory", 3 => "exec", 4 => "branch_edge",
                5 => "syscall", 6 => "context_change", 7 => "module_load",
                8 => "module_unload", _ => "unknown",
            },
        },
    })
}

fn parse_u64(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|_| format!("bad hex value: {text}"))
    } else {
        trimmed.parse::<u64>().map_err(|_| format!("bad number: {text}"))
    }
}

fn to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

/// Runtime architecture from a PING reply, falling back to x64 when the agent
/// predates the `arch` extension (the pre-arch behavior was x64-only).
fn runtime_arch(client: &mut Client) -> Result<u32, String> {
    let arch = client
        .ping_full()
        .map_err(|e| e.to_string())?
        .arch
        .unwrap_or(proto::ARCH_X64);
    Ok(arch)
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() % 2 != 0 {
        return Err("hex string needs an even length".to_string());
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|_| "bad hex data".to_string()))
        .collect()
}

fn run_command(words: &[&str], default_limit: u64, client: &mut Client) -> Result<serde_json::Value, String> {
    let command = words.first().copied().unwrap_or("");
    let arg = |index: usize| -> Result<&str, String> {
        words.get(index).copied().ok_or_else(|| format!("{command} needs argument {index}"))
    };
    match command {
        "ping" => {
            let (major, minor, pid, total) = client.ping().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "abi": [major, minor], "pid": pid, "total": total,
            }))
        }
        "counters" => {
            let (total, dropped, capacity, kinds) =
                client.counters().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "total": total, "dropped": dropped, "capacity": capacity,
                "hook_regs": kinds[0], "memory": kinds[1], "exec": kinds[2],
                "branch_edge": kinds[3], "syscall": kinds[4],
                "context_change": kinds[5], "module_load": kinds[6],
                "module_unload": kinds[7],
            }))
        }
        "events" => {
            let limit = if words.len() > 1 {
                parse_u64(words[1])?
            } else {
                default_limit
            };
            let (total, events) = client.ring_newest(limit).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "total": total,
                "count": events.len(),
                "events": events.iter().map(event_json).collect::<Vec<_>>(),
            }))
        }
        "stop" => {
            let stopped = client.stop().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "stopped": stopped }))
        }
        "resume" => {
            let resumed = client.resume().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "resumed": resumed }))
        }
        "read" => {
            let address = parse_u64(arg(1)?)?;
            let size = parse_u64(arg(2)?)?;
            let data = client.read_memory(address, size).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "address": address, "copied": data.len(), "data": to_hex(&data),
            }))
        }
        "write" => {
            let address = parse_u64(arg(1)?)?;
            let data = from_hex(arg(2)?)?;
            let written = client.write_memory(address, &data).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "address": address, "written": written }))
        }
        "bp" => {
            let address = parse_u64(arg(1)?)?;
            let id = client.bp_set(address).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "id": id, "address": address }))
        }
        "bps" => {
            let (stopped, hit_tid, hit_addr, stop_gen, entries) =
                client.bp_list().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "stopped": stopped,
                "hit_tid": hit_tid,
                "hit_address": hit_addr,
                "stop_gen": stop_gen,
                "count": entries.len(),
                "breakpoints": entries.iter().map(|(id, address, hits)| serde_json::json!({
                    "id": id, "address": address, "hits": hits,
                })).collect::<Vec<_>>(),
            }))
        }
        "bc" => {
            let id = parse_u64(arg(1)?)? as u32;
            let removed = client.bp_remove(id).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "removed": removed }))
        }
        "modules" => {
            let entries = client.modules().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "count": entries.len(),
                "modules": entries.iter().map(|(low, high, main, name)| serde_json::json!({
                    "low": low, "high": high, "main": main, "name": name,
                })).collect::<Vec<_>>(),
            }))
        }
        "threads" => {
            let ids = client.threads().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "count": ids.len(), "thread_ids": ids }))
        }
        "region" | "memregion" => {
            let address = parse_u64(arg(1)?)?;
            let region = client.memory_region(address).map_err(|e| e.to_string())?;
            Ok(match region {
                Some(r) => serde_json::json!({
                    "address": address, "base": r.base, "size": r.size,
                    "end": r.base.saturating_add(r.size),
                    "allocation_base": r.allocation_base,
                    "protect": r.protect, "state": r.state, "type": r.kind,
                }),
                None => serde_json::json!({ "address": address, "mapped": false }),
            })
        }
        "context" => {
            let tid = parse_u64(arg(1)?)? as u32;
            let arch = runtime_arch(client)?;
            let pairs = client.context_get(tid).map_err(|e| e.to_string())?;
            let map: serde_json::Map<String, serde_json::Value> = pairs
                .iter()
                .map(|(reg, value)| (registers::reg_name(arch, *reg), serde_json::json!(format!("0x{value:016x}"))))
                .collect();
            Ok(serde_json::json!({ "thread_id": tid, "registers": map }))
        }
        "setreg" => {
            let tid = parse_u64(arg(1)?)? as u32;
            let name = arg(2)?;
            let arch = runtime_arch(client)?;
            let reg = registers::reg_id(arch, name)
                .ok_or_else(|| format!("unknown register: {name}"))?;
            let value = parse_u64(arg(3)?)?;
            client.context_set(tid, reg, value).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "thread_id": tid, "register": name, "value": value }))
        }
        "engine" => {
            let kind = parse_u64(arg(1)?)? as u32;
            let on = match arg(2)? {
                "on" | "1" | "true" => true,
                "off" | "0" | "false" => false,
                other => return Err(format!("bad on/off: {other}")),
            };
            client.engine_set(kind, on).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "engine": kind, "enabled": on }))
        }
        "exc" => {
            if words.len() < 2 {
                let (enabled, code, pending) = client.exc_policy_get().map_err(|e| e.to_string())?;
                return Ok(serde_json::json!({
                    "enabled": enabled, "exception_code": code, "pending": pending,
                }));
            }
            let (enabled, code) = match words[1] {
                "off" => (false, 0u32),
                "all" => (true, 0u32),
                code_text => (true, parse_u64(code_text)? as u32),
            };
            client.exc_policy_set(enabled, code).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "enabled": enabled, "exception_code": code }))
        }
        "si" | "so" => {
            let tid = parse_u64(arg(1)?)? as u32;
            let over = command == "so";
            let ok = client.step(tid, over).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "thread_id": tid, "over": over, "ok": ok }))
        }
        "disasm" => {
            let address = parse_u64(arg(1)?)?;
            let count = if words.len() > 2 { parse_u64(words[2])? } else { 16 };
            let rows = client.disasm(address, count).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "count": rows.len(),
                "insns": rows.iter().map(|(addr, size, kind, text, bytes, target)| serde_json::json!({
                    "address": format!("0x{addr:x}"), "size": size, "kind": kind,
                    "text": text, "bytes": to_hex(bytes),
                    "target": format!("0x{target:x}"),
                })).collect::<Vec<_>>(),
            }))
        }
        "resolve" => {
            let mut addresses = Vec::new();
            for word in &words[1..] {
                addresses.push(parse_u64(word)?);
            }
            let resolved = client.resolve(&addresses).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "results": addresses.iter().zip(resolved.iter()).map(|(addr, r)| serde_json::json!({
                    "address": addr,
                    "display": r.display(),
                })).collect::<Vec<_>>(),
            }))
        }
        "exports" => {
            let module = arg(1)?;
            let entries = client.exports(module).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "count": entries.len(),
                "exports": entries.iter().map(|(address, name)| serde_json::json!({
                    "address": format!("0x{address:x}"), "name": name,
                })).collect::<Vec<_>>(),
            }))
        }
        "hook" => {
            let address = parse_u64(arg(1)?)?;
            let ok = client.hook_set(address).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "hooked": format!("0x{address:x}"), "ok": ok }))
        }
        "hookall" => {
            let module = arg(1)?;
            let entries = client.exports(module).map_err(|e| e.to_string())?;
            let export_count = entries.len();
            let mut seen = HashSet::with_capacity(export_count);
            let mut addresses = Vec::with_capacity(export_count);
            for (address, _name) in entries {
                if seen.insert(address) {
                    addresses.push(address);
                }
            }
            let unique_count = addresses.len();
            let mut armed = 0usize;
            let mut capacity_full = false;
            for address in addresses {
                if !client.hook_set(address).map_err(|e| e.to_string())? {
                    capacity_full = true;
                    break;
                }
                armed += 1;
            }
            Ok(serde_json::json!({
                "module": module,
                "exports": export_count,
                "unique_addresses": unique_count,
                "armed": armed,
                "skipped_aliases": export_count.saturating_sub(unique_count),
                "capacity_full": capacity_full,
            }))
        }
        "hookrule" => {
            let address = parse_u64(arg(1)?)?;
            let set_reg_name = arg(2)?;
            let set_value = parse_u64(arg(3)?)?;
            let arch = runtime_arch(client)?;
            let set_reg = registers::reg_id(arch, set_reg_name)
                .ok_or_else(|| format!("unknown set register for target architecture: {set_reg_name}"))?;
            let (match_reg, match_mask, match_value, match_name) = if words.len() >= 7 {
                let name = words[4];
                let reg = registers::reg_id(arch, name)
                    .ok_or_else(|| format!("unknown match register for target architecture: {name}"))?;
                (reg, parse_u64(words[5])?, parse_u64(words[6])?, Some(name))
            } else if words.len() == 4 {
                (0, 0, 0, None)
            } else {
                return Err("hookrule needs ADDR SETREG VALUE [MATCHREG MASK VALUE] [TID]".to_string());
            };
            let thread_id = if words.len() >= 8 {
                parse_u64(words[7])? as u32
            } else {
                u32::MAX
            };
            let ok = client
                .hook_rule_set(
                    address,
                    thread_id,
                    match_reg,
                    match_mask,
                    match_value,
                    set_reg,
                    set_value,
                )
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "address": format!("0x{address:x}"),
                "set_reg": set_reg_name,
                "set_value": format!("0x{set_value:x}"),
                "match_reg": match_name,
                "match_mask": format!("0x{match_mask:x}"),
                "match_value": format!("0x{match_value:x}"),
                "thread_id": thread_id,
                "ok": ok,
            }))
        }
        "hookrulesclear" => {
            client.hook_rule_clear().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "cleared": true }))
        }
        "hooks" => {
            let addresses = client.hook_list().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "count": addresses.len(),
                "addresses": addresses.iter().map(|a| format!("0x{a:x}")).collect::<Vec<_>>(),
            }))
        }
        "hookdel" => {
            let address = parse_u64(arg(1)?)?;
            client.hook_remove(address).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "removed": format!("0x{address:x}") }))
        }
        "hookclear" => {
            client.hook_clear().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "cleared": true }))
        }
        "syscallfilter" => {
            let (mode, numbers) = match arg(1)? {
                "all" => (0u8, Vec::new()),
                "only" => {
                    let mut numbers = Vec::new();
                    for word in &words[2..] {
                        numbers.push(parse_u64(word)? as u32);
                    }
                    (1u8, numbers)
                }
                other => return Err(format!("bad syscallfilter mode: {other}")),
            };
            let count = numbers.len();
            client.syscall_filter(mode, &numbers).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "mode": if mode == 0 { "all" } else { "only" }, "count": count,
            }))
        }
        "trace" => {
            match words.get(1).copied() {
                Some("start") => {
                    // kinds csv: exec(=exec_bytes), memory|mem_value, branch,
                    // registers, or a raw recordable kind number
                    let kinds_csv = arg(2)?;
                    let lo = parse_u64(arg(3)?)?;
                    let hi = parse_u64(arg(4)?)?;
                    let path = arg(5)?;
                    let mut kinds = Vec::new();
                    for name in kinds_csv.split(',') {
                        let kind = match name.trim() {
                            "exec" | "exec_bytes" => 9,
                            "memory" | "mem" | "mem_value" => 10,
                            "branch" | "branch_edge" => 4,
                            "syscall" | "syscalls" => 5,
                            "exception" | "exceptions" | "context_change" => 6,
                            "registers" | "regs" | "context" | "reg_snapshot" => 13,
                            other => parse_u64(other)? as u32,
                        };
                        kinds.push(kind);
                    }
                    if kinds.is_empty() {
                        return Err("trace start needs at least one kind".to_string());
                    }
                    client
                        .trace_start(&kinds, lo, hi, path)
                        .map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "recording": true, "kinds": kinds,
                        "lo": lo, "hi": hi, "path": path,
                    }))
                }
                Some("start-spec") => {
                    // ranges: comma-separated LO-HI pairs; threads: `all`
                    // or comma-separated Pin thread ids.
                    let kinds_csv = arg(2)?;
                    let ranges_csv = arg(3)?;
                    let threads_csv = arg(4)?;
                    let path = arg(5)?;
                    let mut kinds = Vec::new();
                    for name in kinds_csv.split(',') {
                        let kind = match name.trim() {
                            "exec" | "exec_bytes" => 9,
                            "memory" | "mem" | "mem_value" => 10,
                            "branch" | "branch_edge" => 4,
                            "syscall" | "syscalls" => 5,
                            "exception" | "exceptions" | "context_change" => 6,
                            "registers" | "regs" | "context" | "reg_snapshot" => 13,
                            "exec_plain" => 3,
                            "mem_plain" => 2,
                            other => parse_u64(other)? as u32,
                        };
                        kinds.push(kind);
                    }
                    let mut ranges = Vec::new();
                    for text in ranges_csv.split(',') {
                        let (lo, hi) = text
                            .split_once('-')
                            .ok_or_else(|| format!("bad range: {text}"))?;
                        ranges.push((parse_u64(lo)?, parse_u64(hi)?));
                    }
                    let mut threads = Vec::new();
                    if threads_csv != "all" {
                        for text in threads_csv.split(',') {
                            threads.push(parse_u64(text)? as u32);
                        }
                    }
                    client
                        .trace_start_spec(&kinds, &ranges, &threads, path)
                        .map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "recording": true, "kinds": kinds,
                        "ranges": ranges, "threads": threads, "path": path,
                    }))
                }
                Some("stop") => {
                    let (recorded, dropped) = client.trace_stop().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "recording": false, "recorded": recorded, "dropped": dropped,
                    }))
                }
                Some("extend") => {
                    let mut ranges = Vec::new();
                    for text in arg(2)?.split(',') {
                        let (lo, hi) = text
                            .split_once('-')
                            .ok_or_else(|| format!("bad range: {text}"))?;
                        ranges.push((parse_u64(lo)?, parse_u64(hi)?));
                    }
                    client.trace_extend(&ranges).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "extended": true, "ranges": ranges }))
                }
                Some("status") | None => {
                    let (active, recorded, dropped) =
                        client.trace_status().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "recording": active, "recorded": recorded, "dropped": dropped,
                    }))
                }
                Some(other) => Err(format!("unknown trace subcommand: {other}")),
            }
        }
        "tracest" => {
            let (active, recorded, dropped) =
                client.trace_status().map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "recording": active, "recorded": recorded, "dropped": dropped,
            }))
        }
        "script" => {
            match words.get(1).copied() {
                Some("run") => {
                    let path = arg(2)?;
                    let source = std::fs::read_to_string(path)
                        .map_err(|e| format!("read {path}: {e}"))?;
                    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
                    let id = client.script_load(name, &source).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "id": id, "name": name }))
                }
                Some("off") => {
                    let name = words.get(2).copied().unwrap_or("all");
                    // the server treats an empty name as "unload all"
                    let wire_name = if name == "all" { "" } else { name };
                    client.script_unload(wire_name).map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "unloaded": name }))
                }
                Some("list") | Some("status") | None => {
                    let entries = client.script_list().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "count": entries.len(),
                        "plugins": entries.iter().map(|e| serde_json::json!({
                            "name": e.name, "state": e.state,
                            "delivered": e.delivered, "dropped": e.dropped,
                        })).collect::<Vec<_>>(),
                    }))
                }
                Some("output") => {
                    let follow = words.iter().any(|w| *w == "--follow");
                    if follow {
                        let mut since = 0u64;
                        loop {
                            let (next, lines) = client
                                .script_output(since, 1024)
                                .map_err(|e| e.to_string())?;
                            for line in &lines {
                                println!(
                                    "{}",
                                    serde_json::to_string(&serde_json::json!({
                                        "seq": line.seq, "plugin": line.plugin,
                                        "line": line.line,
                                    }))
                                    .unwrap()
                                );
                            }
                            since = next;
                            std::thread::sleep(Duration::from_millis(500));
                        }
                    } else {
                        let (_next, lines) =
                            client.script_output(0, 1024).map_err(|e| e.to_string())?;
                        Ok(serde_json::json!({
                            "lines": lines.iter().map(|l| serde_json::json!({
                                "seq": l.seq, "plugin": l.plugin, "line": l.line,
                            })).collect::<Vec<_>>(),
                        }))
                    }
                }
                Some(other) => Err(format!("unknown script subcommand: {other}")),
            }
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// Interactive "debug console": one command per line on stdin, JSON answers
/// on stdout. EOF or `quit` exits. Pipe-friendly for scripted sessions.
fn shell(mut client: Client, default_limit: u64) -> i32 {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut limit = default_limit;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&first, rest)) = words.split_first() else {
            continue;
        };
        match first {
            "quit" | "exit" | "q" => break,
            "help" | "?" => {
                println!("commands: ping | counters | events [N] | stop | resume | read A S | region A | write A HEX | bp A | bps | bc ID | modules | threads | context TID | setreg TID REG V | engine KIND on|off | exc [off|all|CODE] | exports MOD | hook A | hookall MOD | hookrule ADDR SETREG VALUE [MATCHREG MASK VALUE] [TID] | hookrulesclear | hooks | hookdel A | hookclear | syscallfilter all|only [N...] | trace start KINDS LO HI PATH | trace start-spec KINDS LO-HI[,LO-HI] THREADS|all PATH | trace extend LO-HI[,LO-HI] | trace stop | trace status | tracest | script run FILE | script off [NAME|all] | script list | script output [--follow] | limit N | quit");
                continue;
            }
            "limit" => {
                match rest.first().and_then(|v| v.parse::<u64>().ok()) {
                    Some(value) => {
                        limit = value;
                        println!("{{\"limit\":{value}}}");
                    }
                    None => println!("{{\"error\":\"usage: limit N\"}}"),
                }
                continue;
            }
            _ => {}
        }
        match run_command(&words, limit, &mut client) {
            Ok(value) => println!("{}", serde_json::to_string(&value).unwrap()),
            Err(error) => println!("{}", serde_json::json!({ "error": error })),
        }
    }
    0
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            usage();
            eprintln!("error: {error}");
            std::process::exit(2);
        }
    };

    // `run` mode: launch the backend for the given target first, then report
    // counters (unless another explicit command was requested).
    let mut backend = None;
    if config.command == "run" || !config.target.is_empty() {
        if config.target.is_empty() {
            eprintln!("run needs a target after `--`");
            std::process::exit(2);
        }
        let options = launch::LaunchOptions {
            pin: config.pin.clone(),
            agent: config.agent.clone(),
            arch: config.arch,
            port: config.port,
            entry_bp: config.entry_bp,
        };
        match launch::launch_for_target(&options, &config.target, STARTUP_TIMEOUT) {
            Ok((child, _port)) => backend = Some(child),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }

    let exit_code = {
        let connected = Client::connect(config.port)
            .map_err(|e| format!("connect 127.0.0.1:{}: {e}", config.port));
        match connected {
            Ok(mut client) => {
                if config.command.is_empty() || config.command == "shell" || config.command == "run" {
                    shell(client, config.limit)
                } else {
                    let mut words: Vec<&str> = vec![config.command.as_str()];
                    words.extend(config.args.iter().map(|s| s.as_str()));
                    match run_command(&words, config.limit, &mut client) {
                        Ok(value) => {
                            println!("{}", serde_json::to_string_pretty(&value).unwrap());
                            0
                        }
                        Err(error) => {
                            eprintln!("error: {error}");
                            1
                        }
                    }
                }
            }
            Err(error) => {
                eprintln!("error: {error}");
                1
            }
        }
    };
    if let Some(child) = backend.as_mut() {
        launch::kill_backend(child);
    }
    std::process::exit(exit_code);
}
