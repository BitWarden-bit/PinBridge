//! Python-provided instruction bytes served by one native Pin fetch callback.
//!
//! Python only replaces declarative byte segments on the scripting thread.
//! Pin's fetch path reads an immutable process-global snapshot without locks,
//! allocation, RPC, or GIL access. Bytes outside configured segments are
//! fetched through Pin's normal raw-code helper so mixed requests work too.

use super::{with_registry, STATE_RUNNING};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use pinbridge_sys::*;

pub const MAX_SEGMENTS: usize = 64;
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct Segment {
    pub start: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct Spec {
    pub segments: Vec<Segment>,
}

struct NativeSegment {
    start: u64,
    end: u64,
    bytes: Box<[u8]>,
}

struct NativePolicy {
    segments: Vec<NativeSegment>,
}

static POLICY: AtomicPtr<NativePolicy> = AtomicPtr::new(core::ptr::null_mut());
static READERS: AtomicUsize = AtomicUsize::new(0);
static REGISTERED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Prevents a publisher from freeing the snapshot borrowed by a fetch call.
/// The counter is cheaper than a mutex and keeps the callback allocation-free.
struct ReaderGuard;

impl ReaderGuard {
    #[inline]
    fn enter() -> Self {
        READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ReaderGuard {
    #[inline]
    fn drop(&mut self) {
        READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn replace_native(policy: NativePolicy) -> u64 {
    let replacement = Box::into_raw(Box::new(policy));
    let previous = POLICY.swap(replacement, Ordering::SeqCst);
    while READERS.load(Ordering::SeqCst) != 0 {
        std::thread::yield_now();
    }
    if !previous.is_null() {
        unsafe { drop(Box::from_raw(previous)) };
    }
    GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

#[inline]
fn containing_or_next(segments: &[NativeSegment], address: u64) -> Result<usize, usize> {
    segments.binary_search_by(|segment| {
        if segment.end <= address {
            core::cmp::Ordering::Less
        } else if segment.start > address {
            core::cmp::Ordering::Greater
        } else {
            core::cmp::Ordering::Equal
        }
    })
}

unsafe fn fetch_original(
    destination: *mut u8,
    address: u64,
    size: usize,
    exception_info: PbExceptionInfoHandle,
) -> usize {
    let mut copied = 0u64;
    let status = pb_pin_fetch_original_code(
        destination.cast::<c_void>(),
        address,
        size as u64,
        exception_info,
        &mut copied,
    );
    if status == PB_OK {
        copied.min(size as u64) as usize
    } else {
        0
    }
}

unsafe extern "C" fn on_fetch(
    buffer: *mut c_void,
    address: u64,
    size: u64,
    exception_info: PbExceptionInfoHandle,
    _user_data: *mut c_void,
) -> u64 {
    if buffer.is_null() || size == 0 {
        return 0;
    }
    let Ok(requested) = usize::try_from(size) else {
        return 0;
    };
    let _reader = ReaderGuard::enter();
    let snapshot = POLICY.load(Ordering::SeqCst);
    if snapshot.is_null() {
        return fetch_original(buffer.cast::<u8>(), address, requested, exception_info) as u64;
    }
    let segments = &(*snapshot).segments;
    let destination = buffer.cast::<u8>();
    let mut copied = 0usize;

    while copied < requested {
        let Some(cursor) = address.checked_add(copied as u64) else {
            break;
        };
        match containing_or_next(segments, cursor) {
            Ok(index) => {
                let segment = &segments[index];
                let offset = (cursor - segment.start) as usize;
                let available = segment.bytes.len() - offset;
                let amount = available.min(requested - copied);
                core::ptr::copy_nonoverlapping(
                    segment.bytes.as_ptr().add(offset),
                    destination.add(copied),
                    amount,
                );
                copied += amount;
            }
            Err(next) => {
                let remaining = requested - copied;
                let gap = segments
                    .get(next)
                    .map(|segment| (segment.start - cursor).min(remaining as u64) as usize)
                    .unwrap_or(remaining);
                let amount = fetch_original(
                    destination.add(copied),
                    cursor,
                    gap,
                    exception_info,
                );
                copied += amount;
                if amount < gap {
                    break;
                }
            }
        }
    }
    copied as u64
}

fn ensure_registered() -> PbStatus {
    if REGISTERED.load(Ordering::Acquire) {
        return PB_OK;
    }
    // Python policy updates run on a Pin internal thread after application
    // start. Pin requires late callback registration to hold its client lock.
    let lock_status = unsafe { pb_pin_lock_client() };
    if lock_status != PB_OK {
        return lock_status;
    }
    let status = unsafe { pb_pin_add_fetch_function(Some(on_fetch), core::ptr::null_mut()) };
    let unlock_status = unsafe { pb_pin_unlock_client() };
    if status == PB_OK {
        REGISTERED.store(true, Ordering::Release);
    }
    if status == PB_OK && unlock_status != PB_OK {
        return unlock_status;
    }
    status
}

fn ranges(policy: &NativePolicy) -> Vec<(u64, u64)> {
    policy
        .segments
        .iter()
        .map(|segment| (segment.start, segment.end))
        .collect()
}

/// Rebuilds the process-global byte map from every running Python plugin.
pub fn publish() -> Result<u64, PbStatus> {
    let snapshot = POLICY.load(Ordering::Acquire);
    let mut flush_ranges = if snapshot.is_null() {
        Vec::new()
    } else {
        ranges(unsafe { &*snapshot })
    };
    let mut specs = with_registry(|registry| {
        registry
            .values()
            .filter(|plugin| plugin.state == STATE_RUNNING)
            .filter_map(|plugin| {
                plugin
                    .code_fetch
                    .as_ref()
                    .map(|spec| (plugin.name.clone(), spec.clone()))
            })
            .collect::<Vec<_>>()
    });
    specs.sort_by(|left, right| left.0.cmp(&right.0));

    let segment_count = specs
        .iter()
        .map(|(_, spec)| spec.segments.len())
        .sum::<usize>();
    let total_bytes = specs
        .iter()
        .flat_map(|(_, spec)| &spec.segments)
        .try_fold(0usize, |total, segment| total.checked_add(segment.bytes.len()))
        .ok_or(PB_ERR_INVALID_ARGUMENT)?;
    if segment_count > MAX_SEGMENTS || total_bytes > MAX_TOTAL_BYTES {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }

    let mut segments = Vec::with_capacity(segment_count);
    for (_, spec) in specs {
        for segment in spec.segments {
            let end = segment
                .start
                .checked_add(segment.bytes.len() as u64)
                .ok_or(PB_ERR_INVALID_ARGUMENT)?;
            if segment.bytes.is_empty() {
                return Err(PB_ERR_INVALID_ARGUMENT);
            }
            segments.push(NativeSegment {
                start: segment.start,
                end,
                bytes: segment.bytes.into_boxed_slice(),
            });
        }
    }
    segments.sort_unstable_by_key(|segment| segment.start);
    if segments
        .windows(2)
        .any(|pair| pair[1].start < pair[0].end)
    {
        return Err(PB_ERR_INVALID_ARGUMENT);
    }

    let replacement = NativePolicy { segments };
    flush_ranges.extend(ranges(&replacement));
    flush_ranges.sort_unstable();
    flush_ranges.dedup();
    if !replacement.segments.is_empty() {
        let status = ensure_registered();
        if status != PB_OK {
            return Err(status);
        }
    }
    let generation = replace_native(replacement);
    unsafe {
        for (start, end) in flush_ranges {
            let status = pb_pin_remove_instrumentation_in_range(start, end);
            if status != PB_OK {
                return Err(status);
            }
        }
    }
    Ok(generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start: u64, bytes: &[u8]) -> NativeSegment {
        NativeSegment {
            start,
            end: start + bytes.len() as u64,
            bytes: bytes.into(),
        }
    }

    #[test]
    fn lookup_finds_containing_segment_or_next_gap_boundary() {
        let segments = vec![segment(0x1000, &[1, 2]), segment(0x2000, &[3, 4, 5])];
        assert_eq!(containing_or_next(&segments, 0x1001), Ok(0));
        assert_eq!(containing_or_next(&segments, 0x1800), Err(1));
        assert_eq!(containing_or_next(&segments, 0x2002), Ok(1));
        assert_eq!(containing_or_next(&segments, 0x3000), Err(2));
    }

    #[test]
    fn adjacent_segments_are_not_overlaps() {
        let segments = [segment(0x1000, &[1, 2]), segment(0x1002, &[3])];
        assert!(!segments
            .windows(2)
            .any(|pair| pair[1].start < pair[0].end));
    }
}
