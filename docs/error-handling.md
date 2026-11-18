# JAHE error handling (proposed)

Compiler 0.7 has compiler diagnostics and a runtime panic/abort entry point, but no source-level `Result`, `Option`, exceptions, or propagation operator. This is a design for later implementation.

## Recoverable failures as values

The standard library defines `Option<T> = None | Some(T)` for expected absence and `Result<T, E> = Ok(T) | Err(E)` for operations that can fail. They are ordinary generic enums, not primitive compiler types. APIs that perform I/O, parsing, allocation, or network access return `Result`; lookup APIs may return `Option` when absence is not exceptional. Error values should carry structured context and can implement a future `Error` trait. Display text is presentation, not machine-readable identity.

Callers handle results using exhaustive `match`. This makes failure paths visible and works without hidden stack unwinding. A future postfix `?` may propagate `Err` (and perhaps `None` only from option-returning contexts) after generic conversion/coherence rules are designed. It must be equivalent to an explicit match and preserve error values; automatic error stringification is forbidden. Whether `?` supports conversion between error types remains open.

## Panic and termination

`panic(message)` is unrecoverable and returns `never`. The runtime writes a best-effort diagnostic and terminates the process; ABI v1 uses abort. Panic is for violated invariants, programmer errors, and unrecoverable runtime failures, not ordinary file/network failure. No exception model or catch/recover construct is planned for the core language. Abort does not guarantee destructors run. A future unwinding model, if any, must not unwind across C ABI and would require cleanup semantics and target support.

Bounds failure, integer overflow, division by zero, and allocation failure need a consistent policy. Design baseline: checked arithmetic/bounds failures invoke panic; fallible allocation APIs return `Result`; infallible allocation may panic/abort. Exact mapping between runtime status and language `Result` is library API work.

## Error API guidelines

- Return `Result<T,E>` for expected environmental or input errors.
- Use `Option<T>` for absence where no diagnostic detail is needed.
- Keep errors structured; include path/operation/context as fields when useful.
- Do not silently discard an error. Explicit `match`, a documented fallback, or propagation is required.
- Use panic only when continuation is not valid or for an explicit fatal API.
- At FFI boundaries, translate error values to the declared foreign representation; never expose compiler-internal enums by accident.

## Dependencies and deferred decisions

Generic enum support and exhaustive matching precede `Option`/`Result`. Trait/interface rules precede common conversion-based `?` propagation. Runtime ABI work must define status mapping and panic strings. Open questions: exact `?` contexts and conversions, panic backtrace policy, whether allocation aborts by default, and how a future unwinding feature interacts with destructors. No `throw`, exceptions, or `try` keyword is selected.
