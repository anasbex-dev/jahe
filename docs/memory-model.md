# JAHE memory model (proposed)

No memory-safety guarantee described here is implemented by compiler 0.7. The present runtime offers raw allocation functions, while the source frontend has no pointer/aggregate type checking or borrow checker. These rules must be enforced before JAHE may call safe source code memory-safe.

## Proposed ownership model

JAHE proposes affine ownership. Every non-`Copy` value has one live owner. Initializing or passing it by value moves it; the old binding becomes unusable. A type may implement the future `Copy` marker only when duplicating its complete value cannot duplicate resource ownership. Primitive numbers, `bool`, and `char` are `Copy`; `String`, `Vec`, and `Box` are move-only by default. Copy eligibility is explicit through a compiler-known trait, not inferred from small size.

At scope exit, live owned values are destroyed once, in reverse initialization order. Assignment first evaluates the right side, then destroys the previous destination value, then installs the new one, with well-defined behavior if evaluation fails. Struct fields and array elements drop in reverse initialization order. Destructors cannot unwind through an `extern "C"` boundary. Panic behavior and destructor policy during abort are process termination; no cleanup guarantee is made after abort.

## Borrowing

`&T` creates a shared borrow and permits reads. Multiple shared borrows may overlap. `&mut T` creates an exclusive borrow and permits mutation; it cannot overlap any other active borrow of that storage. A borrow is valid only while its owner is live. Returning a reference is allowed only when its source lifetime is represented in the function signature and the checker proves the returned reference does not outlive the source. Initially, elided lifetimes may be inferred only for unambiguous receiver/input relationships; explicit lifetime syntax is an open design decision.

Mutating a value while it is shared-borrowed, moving a borrowed value, invalidating an owner before a borrow ends, or creating overlapping exclusive references is rejected in safe code. Interior mutability requires a library primitive with synchronization/aliasing rules; it is not implicit. The language will not claim Rust equivalence until all aliasing, lifetime, variance, destructor, and unsafe interactions are proven sound.

## Stack, heap, and allocation

Local fixed-size values may be stack allocated, but the language does not promise stack placement; optimization and ABI lowering decide placement. `Box<T>` owns one heap allocation. `Vec<T>` owns a growable contiguous buffer. Allocation failure should return a recoverable error for fallible APIs; infallible allocation may invoke the documented aborting allocation failure path. The raw runtime allocator takes size and alignment and returns null on failure. Safe source interfaces must wrap it in an owner that records layout and guarantees exactly one deallocation.

Manual allocation, deallocation, pointer arithmetic, and custom allocators require `unsafe`. The allocator must receive the exact layout of the original allocation. Reallocation failure preserves the old pointer; successful reallocation invalidates it. Deallocating null is a no-op; unknown/double frees are runtime errors under ABI v1. These runtime behaviors do not themselves prevent source-level dangling pointers.

## References and raw pointers

References are non-null, aligned, point to initialized valid values, and obey borrow lifetimes. Raw `*const T` / `*mut T` may be null, dangling, misaligned, or point to uninitialized bytes. Dereferencing requires unsafe and the programmer must establish validity, alignment, initializedness, provenance, and aliasing conditions. Pointer arithmetic is confined to one allocation including one-past-end for comparisons but not dereference. Pointer-to-integer casts are unsafe; integer roundtrips do not create valid references. `usize` width is target pointer width.

Arrays and slices use bounds-checked indexing in safe code. Out-of-range access produces a defined panic/termination or a later checked `Option` API; the exact distinction is open. `str`/`String` contain valid UTF-8 at safe boundaries. Raw byte mutation can invalidate UTF-8 and therefore requires unsafe or an API that validates before constructing `str`.

## Static and global storage

`const` is compile-time data with no mutable storage identity. A future `static` declaration may provide one address-stable global; mutable statics require unsafe access or a synchronization wrapper. Dynamic initialization and module initializer order are excluded. Thread-local storage and atomics are deferred. This keeps module loading deterministic and avoids hidden initialization cycles.

## FFI and ABI

The default JAHE ABI is compiler-private and may change across compiler versions. `extern "C"` declarations use target C ABI scalar mappings and only explicitly ABI-safe layouts. Foreign calls are unsafe because the compiler cannot prove pointer validity, lifetime, or foreign invariants. Safe wrappers convert foreign results into JAHE-owned values and `Result`. No `String`, `Vec`, trait object, closure environment, or Rust-like generic aggregate may cross the C boundary without an explicit stable representation. Current native implementation is Windows x86_64/MSVC and the runtime has a versioned C ABI; source FFI and portable struct layout do not exist.

## Resource safety, leaks, and limits

Affine ownership prevents double destruction and use-after-move in safe code if the checker is sound. Borrow checking prevents dangling references and mutable aliasing under the defined model. It cannot prevent intentional leaks (`mem::forget` or process lifetime), unbounded allocation, unsafe misuse, or external corruption. Cycles of owning references require weak references or explicit cycle-breaking; tracing GC is not part of the initial memory model. Concurrency safety requires ownership transfer or synchronized shared interior mutation and is deferred.

## Soundness gates before implementation claims

Before marking this model implemented, add formal rules and tests for move paths/partial moves, field and array borrows, lifetime inference and return checking, loops/control-flow joins, closures, destructors during early return, panic/unwind semantics, FFI escape, raw pointer provenance, and generic trait bounds. Until then this is a target design only.
