use jahe_runtime::{
    abi::{check_abi_compatibility, jahe_rt_v1_abi_version},
    error::RuntimeStatus,
    memory::{jahe_rt_v1_alloc, jahe_rt_v1_dealloc, jahe_rt_v1_realloc},
    panic::jahe_rt_v1_panic,
    ABI_PREFIX, ABI_VERSION,
};
use std::{process::Command, ptr};

#[test]
fn abi_is_versioned_and_uses_c_calling_convention() {
    let _: extern "C" fn() -> u32 = jahe_rt_v1_abi_version;
    let _: extern "C" fn(u64, u64) -> *mut u8 = jahe_rt_v1_alloc;
    let _: extern "C" fn(*mut u8) -> u32 = jahe_rt_v1_dealloc;
    let _: extern "C" fn(*mut u8, u64, u64) -> *mut u8 = jahe_rt_v1_realloc;
    assert_eq!(ABI_VERSION, 1);
    assert_eq!(ABI_PREFIX, "jahe_rt_v1_");
    assert_eq!(jahe_rt_v1_abi_version(), ABI_VERSION);
    assert!(check_abi_compatibility(ABI_VERSION, ABI_VERSION).is_ok());
    let mismatch = check_abi_compatibility(ABI_VERSION, ABI_VERSION + 1).unwrap_err();
    assert!(mismatch.to_string().contains("incompatible"));
}

#[test]
fn char_print_rejects_non_scalar_values() {
    assert_eq!(
        jahe_runtime::io::jahe_rt_v1_print_char(0xD800),
        jahe_runtime::RuntimeStatus::InvalidArgument.code()
    );
    assert_eq!(
        jahe_runtime::io::jahe_rt_v1_print_char(0x11_0000),
        jahe_runtime::RuntimeStatus::InvalidArgument.code()
    );
}

#[test]
fn allocation_deallocation_and_alignment_are_defined() {
    let pointer = jahe_rt_v1_alloc(128, 64);
    assert!(!pointer.is_null());
    assert_eq!((pointer as usize) % 64, 0);
    // SAFETY: pointer is a live 128-byte allocation returned above.
    unsafe { ptr::write_bytes(pointer, 0x5a, 128) };
    assert_eq!(jahe_rt_v1_dealloc(pointer), RuntimeStatus::Ok.code());
    assert_eq!(
        jahe_rt_v1_dealloc(pointer),
        RuntimeStatus::InvalidPointer.code()
    );
    assert_eq!(
        jahe_rt_v1_dealloc(ptr::null_mut()),
        RuntimeStatus::Ok.code()
    );
    assert_eq!(
        jahe_rt_v1_dealloc(1usize as *mut u8),
        RuntimeStatus::InvalidPointer.code()
    );
}

#[test]
fn reallocation_preserves_bytes_and_invalid_sizes_return_null() {
    let pointer = jahe_rt_v1_alloc(8, 8);
    assert!(!pointer.is_null());
    // SAFETY: pointer refers to 8 writable bytes owned by this test.
    unsafe { ptr::copy_nonoverlapping(b"runtime!".as_ptr(), pointer, 8) };
    let resized = jahe_rt_v1_realloc(pointer, 24, 32);
    assert!(!resized.is_null());
    assert_eq!((resized as usize) % 32, 0);
    // SAFETY: resized has 24 readable bytes and the original first 8 were copied.
    assert_eq!(
        unsafe { std::slice::from_raw_parts(resized, 8) },
        b"runtime!"
    );
    assert_eq!(jahe_rt_v1_dealloc(resized), RuntimeStatus::Ok.code());

    assert!(jahe_rt_v1_alloc(0, 8).is_null());
    assert!(jahe_rt_v1_alloc(8, 3).is_null());
    assert!(jahe_rt_v1_alloc(u64::MAX, 8).is_null());
    assert!(jahe_rt_v1_realloc(1usize as *mut u8, 32, 8).is_null());
}

#[test]
fn panic_entry_aborts_in_a_subprocess_without_unwinding() {
    const CHILD: &str = "JAHE_RUNTIME_PANIC_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let text = b"child panic message";
        // SAFETY: `text` stays alive and readable for the complete call.
        unsafe { jahe_rt_v1_panic(text.as_ptr(), text.len() as u64) };
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "panic_entry_aborts_in_a_subprocess_without_unwinding",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "runtime panic must terminate the process"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("JAHE runtime panic: child panic message")
    );
}
