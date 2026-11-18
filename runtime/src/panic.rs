//! Unrecoverable panic ABI. Messages are borrowed UTF-8 byte slices and are not
//! retained. The process always aborts; Rust unwinding never crosses C ABI.
use std::{
    io::Write,
    panic::{catch_unwind, AssertUnwindSafe},
    process, slice,
};

/// Report an unrecoverable runtime panic and terminate the process.
///
/// # Safety
/// When `length` is non-zero, `message` must be readable for `length` bytes for
/// the duration of the call. The bytes need not be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn jahe_rt_v1_panic(message: *const u8, length: u64) -> ! {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let stderr = std::io::stderr();
        let mut output = stderr.lock();
        let _ = output.write_all(b"JAHE runtime panic: ");
        if message.is_null() {
            let _ = output.write_all(b"<invalid null message>");
        } else if let Ok(length) = usize::try_from(length) {
            // SAFETY: upheld by this exported function's documented pointer contract.
            let bytes = unsafe { slice::from_raw_parts(message, length) };
            let _ = output.write_all(bytes);
        } else {
            let _ = output.write_all(b"<message length out of range>");
        }
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }));
    process::abort()
}
