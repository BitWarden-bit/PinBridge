//! Address resolution (module!symbol) as a protocol-level capability:
//! scripts, the CLI, and the GUI all read symbols through the same RESOLVE
//! opcode. Modules are enumerated through the ABI; PE export tables are
//! parsed out of target memory (safe copy). One-level IAT thunk chase:
//! resolving an address that points at `jmp qword [rip+disp]` follows to the
//! stored import pointer, so import-call thunks report the real API name.
//!
//! Runs on the query-server thread only; no analysis-callback constraints.

use pinbridge_sys::*;

struct Module {
    low: u64,
    high: u64,
    short: String, // file name only (kernel32.dll)
}

struct ExportEntry {
    rva: u32,
    name: String,
}

struct Resolver {
    modules: Vec<Module>, // sorted by low
    exports: crate::TlsFreeMap<u64, Vec<ExportEntry>>, // by module low, sorted by rva
    failed: crate::TlsFreeSet<u64>, // modules whose export parse failed (don't retry)
}

static mut RESOLVER: Option<Resolver> = None;

/// Serializes resolver access: both the query-server thread and the script
/// host thread resolve symbols. Never used in analysis callbacks, so a
/// std mutex is fine here.
static RESOLVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn resolver() -> &'static mut Resolver {
    unsafe {
        let slot = &mut *core::ptr::addr_of_mut!(RESOLVER);
        if slot.is_none() {
            *slot = Some(Resolver {
                modules: Vec::new(),
                exports: crate::new_map(),
                failed: crate::new_set(),
            });
        }
        slot.as_mut().unwrap()
    }
}

fn read_mem(address: u64, size: usize) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; size];
    let mut copied: u64 = 0;
    unsafe {
        pb_pin_safe_copy(
            buffer.as_mut_ptr() as *mut core::ffi::c_void,
            address,
            size as u64,
            &mut copied,
        );
    }
    if copied as usize != size {
        return None;
    }
    Some(buffer)
}

fn read_u16(address: u64) -> Option<u16> {
    Some(u16::from_le_bytes(read_mem(address, 2)?.try_into().ok()?))
}

fn read_u32(address: u64) -> Option<u32> {
    Some(u32::from_le_bytes(read_mem(address, 4)?.try_into().ok()?))
}

fn read_u64(address: u64) -> Option<u64> {
    Some(u64::from_le_bytes(read_mem(address, 8)?.try_into().ok()?))
}

/// Fresh module snapshot (cheap: ~10 images). Keeps exports cached by base.
fn refresh_modules(r: &mut Resolver) {
    let mut modules = Vec::new();
    unsafe {
        let mut img = PbImgHandle { opaque: 0 };
        if pb_app_img_head(&mut img) != PB_OK {
            return;
        }
        let mut valid: u8 = 0;
        pb_img_valid(img, &mut valid);
        while valid != 0 && modules.len() < 512 {
            let mut low: u64 = 0;
            let mut high: u64 = 0;
            pb_img_low_address(img, &mut low);
            pb_img_high_address(img, &mut high);
            let mut name_buf = [0 as std::os::raw::c_char; 512];
            let mut needed: u64 = 0;
            let name = if pb_img_name(img, name_buf.as_mut_ptr(), 512, &mut needed) == PB_OK {
                std::ffi::CStr::from_ptr(name_buf.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };
            let short = name
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&name)
                .to_string();
            modules.push(Module { low, high, short });
            let mut next = PbImgHandle { opaque: 0 };
            if pb_img_next(img, &mut next) != PB_OK {
                break;
            }
            img = next;
            valid = 0;
            pb_img_valid(img, &mut valid);
        }
    }
    modules.sort_by_key(|m| m.low);
    r.modules = modules;
}

/// Parses the PE export table of the module at `base` (once, then cached).
fn exports_of(r: &mut Resolver, base: u64) -> Option<&Vec<ExportEntry>> {
    if r.failed.contains(&base) {
        return None;
    }
    if !r.exports.contains_key(&base) {
        match parse_exports(base) {
            Some(entries) => {
                r.exports.insert(base, entries);
            }
            None => {
                r.failed.insert(base);
                return None;
            }
        }
    }
    r.exports.get(&base)
}

fn parse_exports(base: u64) -> Option<Vec<ExportEntry>> {
    let header = read_mem(base, 0x40)?;
    let pe_off = u32::from_le_bytes(header[0x3c..0x40].try_into().ok()?) as u64;
    let magic = read_u16(base + pe_off + 24)?; // optional header magic
    if magic != 0x20b {
        return None; // PE32+ only
    }
    let export_rva = read_u32(base + pe_off + 24 + 112)? as u64; // data dir[0]
    if export_rva == 0 {
        return None;
    }
    let dir = read_mem(base + export_rva, 40)?;
    let num_funcs = u32::from_le_bytes(dir[20..24].try_into().ok()?) as u64;
    let num_names = u32::from_le_bytes(dir[24..28].try_into().ok()?) as u64;
    let funcs_rva = u32::from_le_bytes(dir[28..32].try_into().ok()?) as u64;
    let names_rva = u32::from_le_bytes(dir[32..36].try_into().ok()?) as u64;
    let ords_rva = u32::from_le_bytes(dir[36..40].try_into().ok()?) as u64;
    if num_names == 0 || num_names > 65536 || num_funcs > 65536 {
        return None;
    }
    let name_rvas = read_mem(base + names_rva, (num_names * 4) as usize)?;
    let ords = read_mem(base + ords_rva, (num_names * 2) as usize)?;
    let mut out = Vec::with_capacity(num_names as usize);
    for i in 0..num_names as usize {
        let name_rva =
            u32::from_le_bytes(name_rvas[i * 4..i * 4 + 4].try_into().ok()?) as u64;
        let ord = u16::from_le_bytes(ords[i * 2..i * 2 + 2].try_into().ok()?) as u64;
        if ord >= num_funcs {
            continue;
        }
        let func_rva = read_u32(base + funcs_rva + ord * 4)?;
        if func_rva == 0 {
            continue;
        }
        let raw = read_mem(base + name_rva, 128)?;
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let name = String::from_utf8_lossy(&raw[..end]).into_owned();
        if !name.is_empty() {
            out.push(ExportEntry { rva: func_rva, name });
        }
    }
    out.sort_by_key(|e| e.rva);
    Some(out)
}

pub struct Resolution {
    pub kind: u8, // 0 none, 1 module only, 2 export
    pub base: u64,
    pub offset: u64, // module+off (kind 1) or export+off (kind 2)
    pub module: String,
    pub symbol: String,
}

fn module_of(r: &Resolver, address: u64) -> Option<&Module> {
    // last module with low <= address
    let index = r.modules.partition_point(|m| m.low <= address);
    if index == 0 {
        return None;
    }
    let module = &r.modules[index - 1];
    if address < module.high {
        Some(module)
    } else {
        None
    }
}

fn resolve_inner(r: &mut Resolver, address: u64, depth: u8) -> Resolution {
    let none = || Resolution {
        kind: 0,
        base: 0,
        offset: 0,
        module: String::new(),
        symbol: String::new(),
    };
    let (low, high, short) = match module_of(r, address) {
        Some(m) => (m.low, m.high, m.short.clone()),
        None => return none(),
    };
    let _ = high;
    let off = address - low;
    // export lookup: exact or nearest preceding within a page-ish window
    if let Some(exports) = exports_of(r, low) {
        let index = exports.partition_point(|e| e.rva as u64 <= off);
        if index > 0 {
            let entry = &exports[index - 1];
            let delta = off - entry.rva as u64;
            if delta <= 0x2000 {
                return Resolution {
                    kind: 2,
                    base: low,
                    offset: delta,
                    module: short,
                    symbol: entry.name.clone(),
                };
            }
        }
    }
    // IAT thunk chase: `jmp qword [rip+disp]` -> stored import pointer
    if depth == 0 {
        if let Some(bytes) = read_mem(address, 6) {
            if bytes[0] == 0xFF && bytes[1] == 0x25 {
                let disp = i32::from_le_bytes(bytes[2..6].try_into().unwrap()) as i64;
                let slot = (address + 6).wrapping_add(disp as u64);
                if let Some(target) = read_u64(slot) {
                    if target != address {
                        return resolve_inner(r, target, depth + 1);
                    }
                }
            }
        }
    }
    Resolution {
        kind: 1,
        base: low,
        offset: off,
        module: short,
        symbol: String::new(),
    }
}

/// In-process single-address resolution (script host, CLI helpers).
/// Same machinery as the RESOLVE opcode, minus the wire format.
pub fn resolve_one(address: u64) -> Resolution {
    let _guard = RESOLVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let r = resolver();
    refresh_modules(r);
    resolve_inner(r, address, 0)
}

/// Drops all cached state for the module at `base`: the exports cache entry
/// AND the `failed` poison (which otherwise lasts until process end). Called
/// from the IMG load/unload callbacks (modules.rs): after an unload the data
/// is stale, and a load at the same base is a *different* image.
pub fn invalidate(base: u64) {
    let _guard = RESOLVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let r = resolver();
    r.exports.remove(&base);
    r.failed.remove(&base);
}

/// EXPORTS: [u16 name_len][module short name] ->
/// [u32 count][count × (u64 addr, u16 name_len, name bytes)] with
/// addr = base + rva, capped at 8192 entries.
/// Failure policy: unknown module or unparseable export table ->
/// Err(STATUS_INTERNAL); the query server normalizes that to an empty body
/// (same as the other Result-returning handlers). A known module with a
/// valid-but-empty table still returns STATUS_OK with count 0.
pub fn handle_exports(payload: &[u8]) -> Result<Vec<u8>, u8> {
    const EXPORTS_MAX: usize = 8192;
    let mut reader = pinbridge_proto::Reader::new(payload);
    let len = reader.u16().ok_or(pinbridge_proto::STATUS_BAD_REQUEST)? as usize;
    let rest = reader.remaining();
    if rest.len() < len {
        return Err(pinbridge_proto::STATUS_BAD_REQUEST);
    }
    let module_name = String::from_utf8_lossy(&rest[..len]).into_owned();
    let _guard = RESOLVE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let r = resolver();
    refresh_modules(r);
    let base = match r
        .modules
        .iter()
        .find(|m| m.short.eq_ignore_ascii_case(&module_name))
        .map(|m| m.low)
    {
        Some(base) => base,
        None => return Err(pinbridge_proto::STATUS_INTERNAL),
    };
    let entries = match exports_of(r, base) {
        Some(entries) => entries,
        None => return Err(pinbridge_proto::STATUS_INTERNAL),
    };
    let count = entries.len().min(EXPORTS_MAX);
    let mut out = Vec::with_capacity(4 + count * 24);
    pinbridge_proto::put_u32(&mut out, count as u32);
    for entry in entries.iter().take(count) {
        pinbridge_proto::put_u64(&mut out, base + entry.rva as u64);
        let name = entry.name.as_bytes();
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name);
    }
    Ok(out)
}

/// Name -> address: "module!Export" (module = file name, case-insensitive;
/// export exact match first, then case-insensitive). Returns the absolute
/// address, or None when the module or the export is unknown.
pub fn resolve_name(spec: &str) -> Option<u64> {
    let (module_name, export_name) = spec.split_once('!')?;
    if module_name.is_empty() || export_name.is_empty() {
        return None;
    }
    let _guard = RESOLVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let r = resolver();
    refresh_modules(r);
    let base = r
        .modules
        .iter()
        .find(|m| m.short.eq_ignore_ascii_case(module_name))
        .map(|m| m.low)?;
    let exports = exports_of(r, base)?;
    exports
        .iter()
        .find(|e| e.name == export_name)
        .or_else(|| exports.iter().find(|e| e.name.eq_ignore_ascii_case(export_name)))
        .map(|e| base + e.rva as u64)
}

/// RESOLVE: [u32 count][count × u64 address] ->
/// [u32 count] × [u8 kind][u64 base][u64 offset][u16 mlen][module][u16 slen][symbol]
pub fn handle_resolve(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let _guard = RESOLVE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut reader = pinbridge_proto::Reader::new(payload);
    let count = reader.u32().ok_or(pinbridge_proto::STATUS_BAD_REQUEST)?;
    if count > 4096 {
        return Err(pinbridge_proto::STATUS_BAD_REQUEST);
    }
    let mut addresses = Vec::with_capacity(count as usize);
    for _ in 0..count {
        addresses.push(reader.u64().ok_or(pinbridge_proto::STATUS_BAD_REQUEST)?);
    }
    let r = resolver();
    refresh_modules(r);
    let mut out = Vec::with_capacity(64 + count as usize * 24);
    pinbridge_proto::put_u32(&mut out, count);
    for address in addresses {
        let res = resolve_inner(r, address, 0);
        out.push(res.kind);
        pinbridge_proto::put_u64(&mut out, res.base);
        pinbridge_proto::put_u64(&mut out, res.offset);
        let module = res.module.as_bytes();
        out.extend_from_slice(&(module.len() as u16).to_le_bytes());
        out.extend_from_slice(module);
        let symbol = res.symbol.as_bytes();
        out.extend_from_slice(&(symbol.len() as u16).to_le_bytes());
        out.extend_from_slice(symbol);
    }
    Ok(out)
}
