//! Typed `println` services. JAHE strings use borrowed, NUL-terminated UTF-8
//! pointers for this initial ABI; string literals are embedded in the object.
use crate::error::RuntimeStatus;
use std::{
    ffi::CStr,
    io::{self, Write},
    os::raw::c_char,
    panic::{catch_unwind, AssertUnwindSafe},
};

fn output(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        if write(&mut lock).is_ok() {
            RuntimeStatus::Ok.code()
        } else {
            RuntimeStatus::IoError.code()
        }
    }))
    .unwrap_or(RuntimeStatus::InternalError.code())
}

macro_rules! print_number {
    ($name:ident, $ty:ty) => {
        #[no_mangle]
        pub extern "C" fn $name(value: $ty) -> u32 {
            output(|out| writeln!(out, "{value}"))
        }
    };
}

print_number!(jahe_rt_v1_print_i32, i32);
print_number!(jahe_rt_v1_print_i64, i64);
print_number!(jahe_rt_v1_print_f32, f32);
print_number!(jahe_rt_v1_print_f64, f64);

/// Prints one Unicode scalar followed by a newline. Invalid scalar values return `InvalidArgument`.
#[no_mangle]
pub extern "C" fn jahe_rt_v1_print_char(value: u32) -> u32 {
    let Some(character) = char::from_u32(value) else {
        return RuntimeStatus::InvalidArgument.code();
    };
    output(|out| writeln!(out, "{character}"))
}

#[no_mangle]
pub extern "C" fn jahe_rt_v1_print_bool(value: u8) -> u32 {
    output(|out| writeln!(out, "{}", value != 0))
}

/// `value` must point to a readable NUL-terminated UTF-8 string for the
/// duration of the call. The function borrows it and never retains the pointer.
#[no_mangle]
pub unsafe extern "C" fn jahe_rt_v1_print_str(value: *const c_char) -> u32 {
    if value.is_null() {
        return RuntimeStatus::InvalidArgument.code();
    }
    // SAFETY: upheld by this exported function's documented pointer contract.
    let bytes = unsafe { CStr::from_ptr(value) }.to_bytes();
    output(|out| {
        out.write_all(bytes)?;
        out.write_all(b"\n")
    })
}
