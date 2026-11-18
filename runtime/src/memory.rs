//! Explicit raw allocation ABI. The runtime tracks live pointers so invalid and
//! duplicate releases are reported instead of passed to the system allocator.
use crate::error::RuntimeStatus;
use std::{
    alloc::{alloc, dealloc, Layout},
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
    sync::{Mutex, MutexGuard, OnceLock},
};

static ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Layout>>> = OnceLock::new();

fn allocations() -> MutexGuard<'static, HashMap<usize, Layout>> {
    ALLOCATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn requested_layout(size: u64, alignment: u64) -> Option<Layout> {
    let size = usize::try_from(size).ok()?;
    let alignment = usize::try_from(alignment).ok()?;
    if size == 0 {
        return None;
    }
    Layout::from_size_align(size, alignment).ok()
}

fn allocate(layout: Layout) -> *mut u8 {
    // SAFETY: `layout` was constructed by `Layout::from_size_align` and has a
    // non-zero size. The returned pointer is recorded before it is exposed.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        return ptr::null_mut();
    }

    let mut live = allocations();
    if live.try_reserve(1).is_err() {
        // SAFETY: `pointer` is the live allocation created above with `layout`.
        unsafe { dealloc(pointer, layout) };
        return ptr::null_mut();
    }
    live.insert(pointer as usize, layout);
    pointer
}

/// Allocate `size` bytes with the requested power-of-two alignment.
///
/// A zero size, invalid alignment, unrepresentable request, or allocation
/// failure returns null. Returned memory is uninitialized and owned by the
/// caller until `jahe_rt_v1_dealloc` or a successful `jahe_rt_v1_realloc`.
#[no_mangle]
pub extern "C" fn jahe_rt_v1_alloc(size: u64, alignment: u64) -> *mut u8 {
    catch_unwind(AssertUnwindSafe(|| {
        requested_layout(size, alignment).map_or(ptr::null_mut(), allocate)
    }))
    .unwrap_or(ptr::null_mut())
}

/// Release a live allocation. Null is accepted. Unknown pointers and duplicate
/// releases return `InvalidPointer` without being passed to the allocator.
#[no_mangle]
pub extern "C" fn jahe_rt_v1_dealloc(pointer: *mut u8) -> u32 {
    if pointer.is_null() {
        return RuntimeStatus::Ok.code();
    }
    catch_unwind(AssertUnwindSafe(|| {
        let layout = allocations().remove(&(pointer as usize));
        match layout {
            Some(layout) => {
                // SAFETY: the allocation registry only records pointers returned
                // by `alloc` and retains the exact layout used for each pointer.
                unsafe { dealloc(pointer, layout) };
                RuntimeStatus::Ok.code()
            }
            None => RuntimeStatus::InvalidPointer.code(),
        }
    }))
    .unwrap_or(RuntimeStatus::InternalError.code())
}

/// Resize a live allocation. A null pointer acts like allocation. A zero size
/// releases a live pointer and returns null. On failure the old allocation is
/// retained and null is returned. Alignment is always explicit.
#[no_mangle]
pub extern "C" fn jahe_rt_v1_realloc(
    pointer: *mut u8,
    new_size: u64,
    new_alignment: u64,
) -> *mut u8 {
    catch_unwind(AssertUnwindSafe(|| {
        if pointer.is_null() {
            return requested_layout(new_size, new_alignment).map_or(ptr::null_mut(), allocate);
        }
        if new_size == 0 {
            let _ = jahe_rt_v1_dealloc(pointer);
            return ptr::null_mut();
        }
        let Some(new_layout) = requested_layout(new_size, new_alignment) else {
            return ptr::null_mut();
        };

        let mut live = allocations();
        let Some(old_layout) = live.get(&(pointer as usize)).copied() else {
            return ptr::null_mut();
        };
        if live.try_reserve(1).is_err() {
            return ptr::null_mut();
        }
        // SAFETY: `new_layout` is valid, non-zero, and does not overlap the still
        // live old allocation. A null result leaves the old allocation intact.
        let replacement = unsafe { alloc(new_layout) };
        if replacement.is_null() {
            return ptr::null_mut();
        }
        // SAFETY: both blocks are live and non-overlapping. Copy only the bytes
        // that fit in the new block; the old block remains live during copying.
        unsafe {
            ptr::copy_nonoverlapping(
                pointer,
                replacement,
                old_layout.size().min(new_layout.size()),
            );
        }
        live.remove(&(pointer as usize));
        live.insert(replacement as usize, new_layout);
        // SAFETY: `old_layout` is the exact layout recorded for this pointer.
        unsafe { dealloc(pointer, old_layout) };
        replacement
    }))
    .unwrap_or(ptr::null_mut())
}
