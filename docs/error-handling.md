# JAHE error handling (designed; not implemented)

Compiler 0.7 has compile-time diagnostics and an aborting runtime panic entry point. It has no source `Option`, `Result`, match, or propagation operator. This specification defines a value-based error model.

## Recoverable errors

`Option<T> = None | Some(T)` represents expected absence. `Result<T,E> = Ok(T) | Err(E)` represents an operation failure. Both are generic standard-library enums, not primitive types. I/O, parsing, filesystem, networking, and fallible allocation return `Result`; lookup returns `Option` when no additional failure context is useful. Errors carry structured data; formatting is separate and cannot be used as a machine-readable discriminator.

`match` is the base handling mechanism. It is exhaustive for enum results; guarded arms do not count toward exhaustiveness. `?` is postfix error propagation with exact type rules:

- In a function returning `Result<T,E>`, `expr?` requires `expr: Result<U,E>` with exactly the same `E`. `Ok(value)` unwraps to `U`; `Err(error)` returns it from the current function.
- In a function returning `Option<T>`, `expr?` requires `expr: Option<U>`. `Some(value)` unwraps; `None` returns from the current function.
- No implicit error conversion occurs. To change errors, explicitly map/match into the target error type.
- `?` in `void`, scalar, or mismatched Result/Option-returning functions is a compile-time diagnostic. `?` is not optional member access.

This exact-match rule avoids hidden trait conversions and makes propagation easy to type-check before a richer conversion trait exists. Its cost is explicit mapping at API boundaries.

`Result<T,E>` and `Option<T>` are must-use: a value must be matched/otherwise consumed, returned, propagated with `?`, or explicitly discarded. The single explicit discard form is `let _ = operation();`. `_` is not a local binding; it is also a wildcard pattern in `match`. There is no separate `discard` statement. The discard form applies to any expression type, but discarding `Result`/`Option` must be intentional and remains visible in source. Unused ordinary non-void expression results produce a warning; ordinary expression statements are valid when they have type `void`. A function's own return value must likewise be consumed, returned, matched, propagated, or explicitly discarded at its call site when it is must-use.

## Panic, cleanup, and reporting

`panic(message)` has type `never`, prints a best-effort diagnostic with source/runtime context, and aborts the process. It does not unwind; destructors are not run after panic. No catch/recover, exceptions, `throw`, or `try` construct is part of the language. This is simpler and ABI-safe, though it cannot recover resources after fatal failure. Ordinary environmental failures must use `Result`.

Checked integer arithmetic, invalid numeric casts, bounds violations, and explicit fatal invariant failures panic. Fallible allocation returns `Result`; an explicitly infallible API may panic/abort on OOM. Compiler diagnostics (syntax/type/name errors) are not runtime errors and prevent code generation.

## Runtime and native ABI boundary

The C runtime ABI does not expose JAHE `Result`/`Option` enum layout. Runtime entry points return fixed-width status integers or null pointers. A safe source wrapper maps known status values to structured errors; unknown status becomes an `InternalRuntimeError`. Native generated calls must check status or pointer failure according to the imported function contract. A panic aborts before crossing the ABI; no unwinding or foreign exception may cross into JAHE. External C status/out parameters are converted to `Result` only by explicit wrappers.

The runtime ABI v1 output functions return status; current compiler-generated native code traps on a nonzero status. This behavior is implemented for bootstrap `println`; generalized `Result` mapping is a future source-library feature.

## Diagnostics and prerequisites

Required diagnostics include non-exhaustive match, invalid `?` context, mismatched error type, ignored must-use `Result`/`Option`, invalid discard syntax, and incompatible FFI error mapping. Unused ordinary non-void values are warnings, not errors. Generic enum support and match are prerequisites for library `Option`/`Result`; ownership/drop semantics precede safe resource errors; module and runtime ABI resolution precede cross-module wrappers. No exception or unwinding design is planned.
