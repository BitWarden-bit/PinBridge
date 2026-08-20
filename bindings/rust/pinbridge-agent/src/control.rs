//! Control plane handlers: stop/resume/read/write/modules.
//! Runs on the query-server internal thread (never an application thread).

use core::ffi::c_void;
use pinbridge_proto as proto;
use pinbridge_sys::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub static STOPPED: AtomicBool = AtomicBool::new(false);

extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetProcessHeaps(count: u32, heaps: *mut *mut c_void) -> u32;
    fn VirtualQuery(
        address: *const c_void,
        buffer: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
    fn WriteProcessMemory(
        process: *mut c_void,
        base_address: *mut c_void,
        buffer: *const c_void,
        size: usize,
        written: *mut usize,
    ) -> i32;
    fn FlushInstructionCache(process: *mut c_void, base: *const c_void, size: usize) -> i32;
}

#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    #[cfg(target_pointer_width = "64")]
    partition_id: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

#[derive(Copy, Clone)]
pub struct MemoryRegion {
    pub base: u64,
    pub size: u64,
    pub allocation_base: u64,
    pub allocation_protect: u32,
    pub protect: u32,
    pub state: u32,
    pub kind: u32,
}

pub struct ModuleSection {
    pub address: u64,
    pub size: u64,
    pub kind: u32,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub mapped: bool,
    pub name: String,
}

pub struct ModuleLayout {
    pub low: u64,
    pub high: u64,
    pub entry: u64,
    pub mapped_size: u64,
    pub image_type: u32,
    pub is_main: bool,
    pub name: String,
    pub sections: Vec<ModuleSection>,
}

/// Same-process VirtualQuery primitive. Unlike the loopback control request,
/// this can be used by a synchronous Python decision callback while the
/// application thread is waiting for that decision.
pub fn memory_region(address: u64) -> Option<MemoryRegion> {
    let mut info = MemoryBasicInformation {
        base_address: core::ptr::null_mut(),
        allocation_base: core::ptr::null_mut(),
        allocation_protect: 0,
        #[cfg(target_pointer_width = "64")]
        partition_id: 0,
        region_size: 0,
        state: 0,
        protect: 0,
        kind: 0,
    };
    let found = unsafe {
        VirtualQuery(
            address as *const c_void,
            &mut info,
            core::mem::size_of::<MemoryBasicInformation>(),
        ) == core::mem::size_of::<MemoryBasicInformation>()
    };
    found.then_some(MemoryRegion {
        base: info.base_address as u64,
        size: info.region_size as u64,
        allocation_base: info.allocation_base as u64,
        allocation_protect: info.allocation_protect,
        protect: info.protect,
        state: info.state,
        kind: info.kind,
    })
}

/// Enumerates the target's real VirtualQuery layout and process heap roots.
/// Free address-space gaps are walked but omitted from the reply so the UI
/// receives only committed/reserved allocations. The hard cap prevents a
/// corrupted or racing VirtualQuery result from producing an unbounded reply.
pub fn memory_map() -> (Vec<MemoryRegion>, Vec<u64>) {
    const MEM_FREE: u32 = 0x1_0000;
    const MAX_REGIONS: usize = 65_536;

    let mut regions = Vec::new();
    let mut address = 0u64;
    for _ in 0..MAX_REGIONS {
        let Some(region) = memory_region(address) else {
            break;
        };
        let Some(next) = region.base.checked_add(region.size) else {
            break;
        };
        if next <= address {
            break;
        }
        if region.state != MEM_FREE {
            regions.push(region);
        }
        address = next;
    }

    let heap_count = unsafe { GetProcessHeaps(0, core::ptr::null_mut()) };
    let mut raw_heaps = vec![core::ptr::null_mut(); heap_count as usize];
    let written = if raw_heaps.is_empty() {
        0
    } else {
        unsafe { GetProcessHeaps(heap_count, raw_heaps.as_mut_ptr()) }.min(heap_count)
    };
    let heaps = raw_heaps
        .into_iter()
        .take(written as usize)
        .filter(|heap| !heap.is_null())
        .map(|heap| heap as u64)
        .collect();
    (regions, heaps)
}

pub fn is_stopped() -> bool {
    STOPPED.load(Ordering::Acquire)
}

pub fn handle_stop() -> Vec<u8> {
    if is_stopped() {
        return vec![1]; // already stopped: idempotent
    }
    let stopped = crate::bp::control_command(crate::bp::CMD_STOP);
    vec![stopped as u8]
}

pub fn handle_resume() -> Vec<u8> {
    if !is_stopped() {
        // Resuming a running application is a Pin contract violation
        // (pinvm assert kills the whole process). Rapid clients can land
        // here right after a step's internal resume — refuse instead.
        return vec![0];
    }
    // Swallow the one replayed breakpoint execution on resume (steps arm the
    // stepper instead and must not touch this).
    crate::bp::arm_resume_skip();
    let resumed = crate::bp::control_command(crate::bp::CMD_RESUME);
    vec![resumed as u8]
}

pub const READ_MEM_MAX: u64 = 65536;

pub fn read_memory(address: u64, size: u64) -> Vec<u8> {
    let mut buffer = vec![0u8; size as usize];
    let mut copied: u64 = 0;
    unsafe {
        pb_pin_safe_copy(
            buffer.as_mut_ptr() as *mut c_void,
            address,
            size,
            &mut copied,
        );
    }
    buffer.truncate(copied as usize);
    buffer
}

pub fn handle_read_mem(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let size = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    if size == 0 || size > READ_MEM_MAX {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let buffer = read_memory(address, size);
    let copied = buffer.len() as u64;
    let mut out = Vec::with_capacity(16 + buffer.len());
    proto::put_u64(&mut out, address);
    proto::put_u64(&mut out, copied);
    out.extend_from_slice(&buffer);
    Ok(out)
}

/// MEMORY_REGION: [u64 address] -> [u8 found][u64 base][u64 size]
/// [u64 allocation_base][u32 allocation_protect][u32 protect][u32 state][u32 type].
pub fn handle_memory_region(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let info = memory_region(address);
    let mut out = Vec::with_capacity(1 + 8 * 3 + 4 * 4);
    out.push(info.is_some() as u8);
    if let Some(info) = info {
        proto::put_u64(&mut out, info.base);
        proto::put_u64(&mut out, info.size);
        proto::put_u64(&mut out, info.allocation_base);
        proto::put_u32(&mut out, info.allocation_protect);
        proto::put_u32(&mut out, info.protect);
        proto::put_u32(&mut out, info.state);
        proto::put_u32(&mut out, info.kind);
    }
    Ok(out)
}

pub const WRITE_MEM_MAX: u64 = 65536;

pub fn handle_write_mem(payload: &[u8]) -> Result<Vec<u8>, u8> {
    let mut reader = proto::Reader::new(payload);
    let address = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)?;
    let len = reader.u64().ok_or(proto::STATUS_BAD_REQUEST)? as usize;
    let data = reader.remaining();
    if len == 0 || len as u64 > WRITE_MEM_MAX || data.len() < len {
        return Err(proto::STATUS_BAD_REQUEST);
    }
    if !is_stopped() {
        // writing a running target is a contract violation (matches the old
        // debugger discipline: stop -> mutate -> resume)
        return Err(proto::STATUS_BAD_REQUEST);
    }
    let mut written: usize = 0;
    let ok = unsafe {
        WriteProcessMemory(
            GetCurrentProcess(),
            address as *mut c_void,
            data.as_ptr() as *const c_void,
            len,
            &mut written,
        )
    };
    if ok != 0 && written > 0 {
        unsafe {
            FlushInstructionCache(GetCurrentProcess(), address as *const c_void, written);
        }
    }
    let mut out = Vec::with_capacity(8);
    proto::put_u64(&mut out, written as u64);
    Ok(out)
}

const MAX_MODULES: usize = 512;
const MAX_SECTIONS_PER_MODULE: usize = 256;

pub fn module_layout() -> Vec<ModuleLayout> {
    let mut entries = Vec::new();
    unsafe {
        let mut img = PbImgHandle { opaque: 0 };
        if pb_app_img_head(&mut img) != PB_OK {
            return entries;
        }
        let mut valid: u8 = 0;
        pb_img_valid(img, &mut valid);
        while valid != 0 && entries.len() < MAX_MODULES {
            let mut low: u64 = 0;
            let mut high: u64 = 0;
            let mut entry: u64 = 0;
            let mut mapped_size: u64 = 0;
            let mut image_type: PbImgType = 0;
            let mut is_main: u8 = 0;
            pb_img_low_address(img, &mut low);
            pb_img_high_address(img, &mut high);
            pb_img_entry_address(img, &mut entry);
            pb_img_size_mapped(img, &mut mapped_size);
            pb_img_type(img, &mut image_type);
            pb_img_is_main_executable(img, &mut is_main);
            let mut name_buf = [0 as std::os::raw::c_char; 512];
            let mut needed: u64 = 0;
            let name = if pb_img_name(img, name_buf.as_mut_ptr(), 512, &mut needed) == PB_OK {
                std::ffi::CStr::from_ptr(name_buf.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };

            let mut sections = Vec::new();
            let mut section = PbSecHandle { opaque: 0 };
            if pb_img_sec_head(img, &mut section) == PB_OK {
                let mut section_valid = 0u8;
                pb_sec_valid(section, &mut section_valid);
                while section_valid != 0 && sections.len() < MAX_SECTIONS_PER_MODULE {
                    let mut address = 0u64;
                    let mut size = 0u64;
                    let mut kind: PbSecType = 0;
                    let mut readable = 0u8;
                    let mut writable = 0u8;
                    let mut executable = 0u8;
                    let mut mapped = 0u8;
                    pb_sec_address(section, &mut address);
                    pb_sec_size(section, &mut size);
                    pb_sec_type(section, &mut kind);
                    pb_sec_is_readable(section, &mut readable);
                    pb_sec_is_writeable(section, &mut writable);
                    pb_sec_is_executable(section, &mut executable);
                    pb_sec_mapped(section, &mut mapped);
                    let mut section_name_buf = [0 as std::os::raw::c_char; 128];
                    let mut section_name_needed = 0u64;
                    let section_name = if pb_sec_name(
                        section,
                        section_name_buf.as_mut_ptr(),
                        section_name_buf.len() as u64,
                        &mut section_name_needed,
                    ) == PB_OK
                    {
                        std::ffi::CStr::from_ptr(section_name_buf.as_ptr())
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        String::new()
                    };
                    sections.push(ModuleSection {
                        address,
                        size,
                        kind,
                        readable: readable != 0,
                        writable: writable != 0,
                        executable: executable != 0,
                        mapped: mapped != 0,
                        name: section_name,
                    });
                    let mut next = PbSecHandle { opaque: 0 };
                    if pb_sec_next(section, &mut next) != PB_OK {
                        break;
                    }
                    section = next;
                    section_valid = 0;
                    pb_sec_valid(section, &mut section_valid);
                }
            }

            entries.push(ModuleLayout {
                low,
                high,
                entry,
                mapped_size,
                image_type,
                is_main: is_main != 0,
                name,
                sections,
            });
            let mut next = PbImgHandle { opaque: 0 };
            if pb_img_next(img, &mut next) != PB_OK {
                break;
            }
            img = next;
            valid = 0;
            pb_img_valid(img, &mut valid);
        }
    }
    entries
}

pub fn modules() -> Vec<(u64, u64, bool, String)> {
    module_layout()
        .into_iter()
        .map(|module| (module.low, module.high, module.is_main, module.name))
        .collect()
}

pub fn handle_modules() -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    let entries = modules();
    proto::put_u32(&mut out, entries.len() as u32);
    for (low, high, is_main, name) in entries {
        proto::put_u64(&mut out, low);
        proto::put_u64(&mut out, high);
        out.push(is_main as u8);
        let bytes = name.as_bytes();
        proto::put_u32(&mut out, bytes.len() as u32);
        out.extend_from_slice(bytes);
    }
    out
}

pub fn handle_memory_map() -> Vec<u8> {
    let (regions, heaps) = memory_map();
    let modules = module_layout();
    let mut out = Vec::with_capacity(regions.len() * 40 + modules.len() * 128 + 64);

    proto::put_u32(&mut out, regions.len() as u32);
    for region in regions {
        proto::put_u64(&mut out, region.base);
        proto::put_u64(&mut out, region.size);
        proto::put_u64(&mut out, region.allocation_base);
        proto::put_u32(&mut out, region.allocation_protect);
        proto::put_u32(&mut out, region.protect);
        proto::put_u32(&mut out, region.state);
        proto::put_u32(&mut out, region.kind);
    }

    proto::put_u32(&mut out, heaps.len() as u32);
    for heap in heaps {
        proto::put_u64(&mut out, heap);
    }

    proto::put_u32(&mut out, modules.len() as u32);
    for module in modules {
        proto::put_u64(&mut out, module.low);
        proto::put_u64(&mut out, module.high);
        proto::put_u64(&mut out, module.entry);
        proto::put_u64(&mut out, module.mapped_size);
        proto::put_u32(&mut out, module.image_type);
        out.push(module.is_main as u8);
        let name = module.name.as_bytes();
        proto::put_u32(&mut out, name.len() as u32);
        out.extend_from_slice(name);
        proto::put_u32(&mut out, module.sections.len() as u32);
        for section in module.sections {
            proto::put_u64(&mut out, section.address);
            proto::put_u64(&mut out, section.size);
            proto::put_u32(&mut out, section.kind);
            let flags = (section.readable as u8)
                | ((section.writable as u8) << 1)
                | ((section.executable as u8) << 2)
                | ((section.mapped as u8) << 3);
            out.push(flags);
            let name = section.name.as_bytes();
            proto::put_u32(&mut out, name.len() as u32);
            out.extend_from_slice(name);
        }
    }
    out
}
