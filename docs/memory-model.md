# JAHE memory model (designed; not implemented)

Compiler 0.7 has no source aggregate/pointer checker or borrow checker. Its separate runtime exposes raw allocation and deallocation entry points, which do not make source programs memory-safe. The following affine model is selected as JAHE's design, subject to soundness tests before any safety claim.

## Ownership and copy/move

Each value is owned by one binding or aggregate field. By-value parameter passing and return transfer ownership. After a move, the old place is unavailable; assigning a new complete value reinitializes it. `Copy` is a compiler-recognized marker implemented only for types whose duplication does not duplicate resource ownership. Numeric primitives, bool, char, raw pointers and immutable references are Copy; mutable references and owning heap types (`String`, `Vec`, `Box`) are move-only. Struct/tuple/array Copy is derived only when all components are Copy and the type has no destructor.

Struct fields may be partially moved when the type has no destructor; moved fields are unusable, remaining fields remain usable, and whole-value use is rejected until all required fields are reinitialized. Tuples and arrays do not support partial moves in the initial design. Assignments evaluate the place (including base/index) first, then the right-hand side, drop the prior initialized destination, and store. Function arguments and tuple/array/struct fields evaluate left to right. These rules support predictable side effects and drop behavior.

## Borrowing and lifetimes

`&'a T` is a shared borrow; any number may overlap and they permit reads. `&'a mut T` is exclusive and permits mutation; it cannot overlap any live borrow of the same storage. A borrow cannot outlive its owner. Moving or dropping borrowed storage, mutating while shared-borrowed, or creating overlapping mutable references is rejected.

Local borrow lifetimes are inferred by control-flow dataflow. Function input references may omit lifetime names when they do not appear in the return type. If a return type contains a reference and there is exactly one borrowed input (counting a receiver), it inherits that lifetime; otherwise declaration must name explicit `'a` parameters and connect each result reference, for example `func first<'a, T>(items: &'a [T]) -> Option<&'a T>`. Lifetime parameters precede type parameters. Elision never chooses between multiple possible inputs. Invalid inference reports source and target borrow spans. Variance and higher-ranked lifetimes are deferred until required and specified.

## Destruction and scopes

Every initialized owning value is dropped exactly once at normal scope exit in reverse initialization order; early `return` and `break` drop exited scopes in that same order. Struct fields drop in reverse declaration order; arrays drop from highest index to lowest. Assignment drops the old value after right-hand evaluation succeeds and before replacing it. `impl Drop for T { func drop(&mut self) { ... } }` is the sole destructor form. A destructor cannot move fields out, cannot be called directly, and must not panic; a panic from a destructor aborts. During process abort, no destructor execution is promised. These constraints prevent double cleanup and simplify unwind behavior.

## Heap, allocation, and ownership across calls

`Box<T>` owns one allocation; `Vec<T>` owns a buffer; `String` owns UTF-8 bytes. Their safe wrappers store the exact allocation/layout and deallocate through the allocator that created the block. Raw `alloc/realloc/dealloc` require unsafe. A zero/invalid allocation returns null; failed reallocation leaves the old block live; successful reallocation invalidates the old pointer. Deallocation must use the same runtime allocator; the current ABI tracks layouts and rejects unknown/double-free pointers. Cross-module calls transfer/borrow by type rules, not by copying an owned pointer implicitly.

Fallible APIs return `Result`; explicit infallible allocation may call panic. Leaks are possible only through process-lifetime retention or an explicit unsafe/forget operation. The design has no tracing GC. Cycles of owning references need `Weak`/manual cycle breaking; weak pointers are a later library type.

## References and raw pointers

Safe references are non-null, aligned, initialized, valid for `T`, and obey lifetimes. `*const T` and `*mut T` may be null, dangling, unaligned, or uninitialized. Dereference, pointer arithmetic, pointer-to-integer conversion, manual allocation, union access, mutable static access, volatile operations, and foreign calls require explicit `unsafe { ... }`. Pointer arithmetic stays within one allocation; one-past-end may be formed but not dereferenced. A pointer integer roundtrip does not restore provenance or validity. Raw pointer operations inside `unsafe func` still require an unsafe block; marking a function unsafe means callers uphold documented preconditions.

Arrays/slices use bounds-checked indexing; a dynamic out-of-range access invokes panic. `str`/`String` must contain valid UTF-8 when viewed safely. Byte mutation that may invalidate UTF-8 is unsafe until validated. Unsafe code remains fully type-checked and cannot make invalid references valid by annotation.

## Interior mutability and concurrency

No implicit interior mutability. `Cell<T>` is planned for `Copy` values with single-threaded mutation and no borrowed references escaping. `Mutex<T>` provides synchronized shared mutation. A future standard library defines unsafe marker traits `Send` (safe to move across threads) and `Sync` (safe to share through immutable references); compiler-derived structural checks reject unsound components, and manual implementations use `unsafe impl`. Thread spawning transfers `Send` inputs; sharing requires `Sync` or synchronization. The initial planned API is `std.thread.spawn(func() -> T) -> Result<JoinHandle<T>, ThreadError>` and `join(handle) -> Result<T, ThreadError>`; joining consumes the handle. Atomics use explicit `Relaxed`, `Acquire`, `Release`, `AcqRel`, or `SeqCst`; `Consume` is not provided. Volatile is not synchronization and always unsafe. These APIs are deferred until the memory model and target runtime can enforce data-race freedom.

## Static storage

`const` has no mutable storage identity. `static` and thread-local variables are deferred. If `static` is added, initialization is compile-time only, mutable access is unsafe unless behind synchronization, and there are no module initialization functions. This avoids order-dependent startup.

## FFI, ABI, and platform boundary

The ordinary JAHE ABI is compiler-private and may change between compiler/runtime versions. `extern "C"` initially permits `void`, fixed-width signed/unsigned integer and float scalars, raw pointers, and pointer-sized integers only when target matches. `bool` crosses as `u8`; `char`, strings, aggregates, generics, enums, trait objects and closures do not cross directly. Borrowed string arguments use `(pointer, byte_length)` and are valid only for the call; returned/owned strings need an explicit future buffer/allocator contract. No foreign function may unwind into JAHE; runtime panic aborts. JAHE error values must map through declared integer status/out parameters, then safe wrappers make `Result`.

Struct/enum layout is not a stable ABI. `repr(C)` is deferred; until then FFI uses opaque pointers for aggregates. Runtime imports use the versioned `jahe_rt_v1_` symbol prefix and exact ABI version; compiler/runtime mismatch is a link/build error. Current native support is Windows x86_64/MSVC only. Linux, macOS, ARM, cross-target ABI and source FFI are not implemented.

## Selected trade-off and implementation gate

Manual memory alone offers control but makes ordinary ownership fragile; tracing GC simplifies reclamation but adds runtime/scheduling costs and weakens predictable latency. JAHE selects affine ownership plus borrowing and deterministic destruction, with unsafe escape hatches and no mandatory collector. This has high compiler-analysis cost, especially for control-flow joins, partial moves, FFI and closures. Before calling the language safe, tests must cover moves, borrow conflicts/lifetimes, loops, drops on all exits, destructors, raw-pointer boundaries, FFI escape, and concurrency auto traits. Until then the model is design-only.
