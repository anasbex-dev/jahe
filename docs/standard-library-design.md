# JAHE standard library architecture (proposed)

The repository's `std/` directory is currently reserved; no JAHE standard library modules exist. `jahe-runtime` is a low-level ABI/runtime crate, not a source standard library. Standard library names below are namespaces under design only.

## Layer boundaries

1. **Compiler built-ins:** syntax, static type rules, target layout query, and only irreducible operations such as `size_of<T>()` when specified. Built-ins are not a general I/O or collection API.
2. **Runtime:** process startup support, versioned ABI, raw allocator, panic termination, and minimal low-level services needed by generated code. It avoids high-level generic types and has a C-compatible boundary.
3. **Standard library:** safe ownership wrappers, strings/collections, formatting, I/O, filesystem, networking, time, threading, synchronization, and math. It is separately documented/tested against a compiler/runtime ABI version.
4. **Third-party packages:** domain libraries such as HTTP, image codecs, audio codecs, and JSON. They use public standard library interfaces and package metadata; they do not become compiler built-ins.

## Proposed namespaces

```text
std.core          Option, Result, CastError, checked casts, fundamental traits
std.mem           Box, allocation-safe wrappers, slices
std.string        String, str utilities, UTF-8 validation
std.collections   Vec, HashMap, HashSet, iterators
std.iter          Iterator and iterator adapters
std.io            readers, writers, formatting, standard streams
std.fs            paths, files, directory operations
std.net           address/socket primitives and errors
std.time          clocks, durations, timers
std.thread        thread creation and joining
std.sync          mutexes, channels, atomics after memory-order design
std.math          checked/common numeric functions
```

`Iterator<T>` is a trait in `std.iter`, not a compiler primitive. `String`, `Vec`, `Box`, `Option`, `Result`, `HashMap`, and `HashSet` are library-defined generic types. Core ownership rules must exist before safe collection APIs are exposed.

Third-party packages might include `net.http`, `net.tcp`, `net.udp`, `image.png`, `image.jpeg`, `audio.wav`, `audio.mp3`, and `json`. Their names are examples, not promised packages. Codec and protocol support does not belong in the compiler or mandatory runtime.

## Versioning and portability

The standard library is versioned with the language edition and runtime ABI, while package APIs have semantic versions. Platform-specific APIs are explicit submodules or capability-gated APIs; portable types should not expose OS handles without wrappers. Native linking should include only runtime pieces actually referenced and the target's system libraries. Cross compilation needs target-specific runtime artifacts; the host runtime must never be silently linked for a different target.

## Implementation order and safeguards

`std.core` owns `Option`, `Result`, `CastError`, and marker traits; `std.mem` owns `Box` and slices; `std.string` owns `String`/UTF-8 utilities; `std.collections` owns vectors/maps/sets; `std.iter` owns `Iterator`; `std.io` and `std.fs` own stream/file error types; `std.net` owns socket/address/error types. `Option` and `Result` are must-use; code must handle, return, propagate, or explicitly discard them with `let _ = expression;`. These module assignments are designed, not present source modules.

First establish primitive/type/layout rules, then ownership, `Option`/`Result`, and UTF-8 string slices. Add allocation-safe `Box` and `Vec` only after drop and borrow checking. Add formatting before ergonomic I/O, then filesystem, networking, time, and synchronization. Concurrency and atomics follow an explicit data-race and memory-order model. Each module needs portable API documentation, platform-specific behavior notes, tests, and a clear distinction between expected errors and panic conditions.

No current source can write `import std.io`; compiler 0.7's native runtime typed `println` integration is a bootstrap service and should not be mistaken for a standard library module.
