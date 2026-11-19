# JAHE Runtime ABI v1

JAHE 0.7 separates the `jahe` compiler from the `jahe-runtime` crate. The runtime has no dependency on compiler internals. Native object generation stays in Cranelift; executable linking includes a static `jahe_runtime.lib` archive and a small Rust entry shim so the Rust linker driver supplies the host CRT and Windows system libraries.

## ABI contract

- ABI version: `1`, exported as `jahe_rt_v1_abi_version() -> u32` and exposed to Rust as `jahe_runtime::ABI_VERSION`.
- Symbol prefix: `jahe_rt_v1_`. Changing an incompatible ABI requires a new versioned symbol prefix.
- Calling convention: `extern "C"` on the Windows x64 target.
- ABI values: fixed-width `u8`, `u32`, `u64`, and raw pointers. No Rust `String`, `Vec`, trait object, Rust enum without `repr`, or Rust calling convention crosses the boundary.
- Compatibility: the compiler checks its required ABI version against the linked runtime crate before code generation. Versioned imports in the generated object make a mismatched static archive fail at link time; that failure is reported as a runtime ABI mismatch.

### Status codes

`RuntimeStatus` is `#[repr(u32)]`: `Ok=0`, `InvalidArgument=1`, `InvalidPointer=2`, `OutOfMemory=3`, `IoError=4`, `InternalError=5`. Runtime print imports return this status. Generated code traps on a non-zero print status; errors are not discarded.

### Memory

```text
jahe_rt_v1_alloc(size: u64, alignment: u64) -> *mut u8
jahe_rt_v1_realloc(pointer: *mut u8, new_size: u64, new_alignment: u64) -> *mut u8
jahe_rt_v1_dealloc(pointer: *mut u8) -> u32
```

Alignment must be non-zero and a power of two. A zero-size allocation, invalid layout, unrepresentable size, or allocation failure returns null. A null pointer passed to deallocation succeeds. Non-null pointers must be live pointers returned by this allocator; unknown and duplicate frees return `InvalidPointer` without being passed to the system allocator. A failed reallocation returns null and preserves the original allocation. Reallocation with size zero releases a valid pointer and returns null. Returned memory is uninitialized and owned by the caller; deallocation or successful reallocation ends that ownership. The runtime records each pointer's exact allocation layout. This foundation does not provide garbage collection, ownership checking, or borrow checking.

### Panic and errors

`jahe_rt_v1_panic(message: *const u8, length: u64) -> !` borrows a byte slice for the call, writes a panic message to stderr when possible, and always calls `process::abort`. It never resumes unwinding across C ABI. The initial runtime has no recoverable language-level error propagation; memory and I/O services return status or null values where specified. An unrecoverable compiler-generated runtime service failure traps the native process.

### I/O

The runtime exports `jahe_rt_v1_print_i32`, `_i64`, `_f32`, `_f64`, `_bool`, `_char`, and `_str`. Numeric helpers take the matching primitive type; boolean takes `u8`; char takes a Unicode scalar code point in `u32` and returns `InvalidArgument` for a surrogate or out-of-range scalar; string takes a borrowed NUL-terminated UTF-8 pointer. The string pointer is not retained; compiler string literals live for the full executable lifetime. Every helper writes one newline and returns a status code. The char export is an additive ABI v1 symbol using an already permitted fixed-width type, so it does not change existing ABI layouts or call contracts. NUL-containing source strings remain rejected by the native backend because this v1 string ABI is NUL-terminated.

## Build and capabilities

The compiler builds the runtime static library offline from the sibling runtime crate during `jahe build`, then links it with the generated COFF object using the detected MSVC toolchain. `jahe runtime-info` reports the ABI, host architecture, implemented memory/panic/println capabilities, and static-library configuration. The executable contains the runtime code and runs without the compiler or interpreter process.

The runtime currently provides allocation, reallocation, deallocation, panic termination, and typed `println`. It does not yet provide string allocation/ownership, general error propagation, stdin, filesystem, networking, collections, garbage collection, or a standard library.
