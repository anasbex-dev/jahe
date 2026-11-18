# JAHE language specification (design baseline)

**Status:** proposed design for user review; not frozen and not implemented wholesale. Every area is classified as implemented, designed, deferred, or experimental. Current compiler support is described only in [language-spec.md](language-spec.md).

## Identity and principles

JAHE is intended to be a statically typed, ahead-of-time compiled systems language with predictable data layout, explicit low-level control, and useful high-level abstractions. The design favors readability, explicit types at public/ABI boundaries, local inference for initialized bindings, private-by-default visibility, structured control flow, and explicit conversions. Syntax is chosen for clarity and maintainability, not to reproduce Rust. Predictable execution and cross-platform compilation are goals, not current claims. Ownership and borrowing are a proposed architecture only; JAHE must not claim memory safety until a sound checker is implemented and tested.

## Implementation status

| Area | Compiler 0.7 today | Designed direction |
|---|---|---|
| Function | `fn`, typed parameters, optional return type | `func`; this design is not parsed |
| Binding | `let`, `let mut` | immutable `let`, mutable `var`, compile-time `const` |
| Types | `i32`, `i64`, `f32`, `f64`, `bool`, `str`, `void` | full scalar family, aggregates, references, generics |
| Modules | one `module` declaration; no import | `module` identity; distinct `import` visibility |
| Control | `if`, `else`, `while`, `return` | adds `for`, `loop`, `match`, `break`, `continue` later |
| Native target | Cranelift, Windows x86_64 COFF/MSVC | additional targets are future work |
| Re-export / packages | no imports or package manager | `pub import`; `jahe.toml` + generated `jahe.lock` |

See [language-spec.md](language-spec.md) for accepted forms, [syntax-reference.md](syntax-reference.md) for the designed surface, and [backend-design.md](backend-design.md)/[runtime-abi.md](runtime-abi.md) for verified implementation facts.

## Keywords, declarations, and scope

Official design keywords are `func`, `return`, `let`, `var`, `const`, `module`, `import`, `as`, `pub`, `if`, `else`, `while`, `for`, `in`, `loop`, `break`, `continue`, `match`, `struct`, `enum`, `impl`, `trait`, `unsafe`, `extern`, `mut`, `self`, `true`, and `false`. `mut` is reserved and accepted only in `&mut T`, `*mut T`, and `&mut self` forms. `self` is reserved and valid as a value only in a method that declares a receiver. `async` and `await` are not reserved keywords; async execution is deferred. Keyword policy and rationale are in [language-design-decisions.md](language-design-decisions.md).

A function declaration has the form `func name(parameters) -> Type { return value; }`; omitted result type means `void`. Parameters are typed. `return;` is only for void functions; `return value;` must match the result. One function namespace per module; no overloading. Functions in one module may recurse mutually after its signatures are collected. Cross-module cycles are errors. Functions are private by default; `pub` exports them. JAHE calls use a compiler-controlled target ABI, not a stable external ABI. Variadic functions are deferred. `extern "C" func` uses a fixed-width C-compatible signature; calling it is unsafe. ABI details are in [runtime-abi.md](runtime-abi.md).

`let name: T = value;` binds an immutable local; `var name: T = value;` binds a mutable local; `const NAME: T = value;` declares a compile-time constant. Initializers are required. `let _ = expression;` explicitly discards a value; `_` is also a wildcard pattern, never a name-bearing binding. `Result` and `Option` are must-use: they must be propagated, returned, matched/otherwise consumed, or explicitly discarded with this form. Other unused non-void results produce a warning; expression statements are permitted for void expressions and warn for other ordinary values. Local annotations may be omitted only when the initializer determines a unique type. No implicit lossy conversions. Lexical blocks scope names; nested shadowing is allowed, same-scope duplicates are errors. Destructuring, mutable global storage and `static` are deferred. A future `static` needs explicit initialization and concurrency rules.

The initial executable entry is a normal, private function `func main() -> i32` with no parameters, located in the source module selected by the package's `entry` field. The compiler treats that designated function as the process entry and maps its `i32` result to the target platform's process exit status. An executable package must designate exactly one entry source and define exactly one matching `main`; it may not be generic or overloaded. A library package omits `entry` and has no process-entry requirement; a library may use `main` for an ordinary private function, which is never selected as an executable entry. No argument vector or `void` entry return is supported initially. Compiler 0.7 uses existing `fn` syntax and its separate single-file CLI conventions.

## Type system

Proposed primitives: `bool`; signed `i8/i16/i32/i64/i128/isize`; unsigned `u8/u16/u32/u64/u128/usize`; `f32/f64`; `char`; `void`; `never`. Fixed integer widths are target-independent; `isize`/`usize` match target pointer width. Alignment and aggregate layout follow the selected target ABI. `char` is a Unicode scalar, not a byte. `void` is absence of a result; `never` has no values and can inhabit a result position because it never returns.

Integer literals infer from context or default to the smallest fitting signed `i32`, `i64`, `i128`; otherwise require an explicit fitting unsigned type or error. Floats default to `f64`. Integer arithmetic is checked in every build; division by zero and signed min divided by -1 panic. Wrapping uses explicit integer `wrapping_add/sub/mul` methods. Floating arithmetic follows IEEE 754. `value as T` is explicit and checked; out-of-range conversions panic. `std.core.try_as(value)` with an expected `Result<T, CastError>` type returns `Result<T, CastError>` for recoverable conversions. No implicit numeric conversion is permitted. Integer-to-float precision loss is allowed only by explicit cast and follows IEEE rounding.

Arrays `[T; N]` are fixed-size inline sequences; slices `&[T]` are pointer-plus-length views; tuples are positional products with `pair.0` projections; structs are nominal records; enums are tagged sums with unit, tuple, or named-field variants. Safe indexing is bounds-checked. Tuple indexes are compile-time checked. Enum matching is exhaustive for enums and bool; integer/char/string matching requires a wildcard. Guarded arms do not contribute to exhaustiveness. Union layout is deferred; reading a non-active member would require unsafe. Recursive aggregates require indirection (`Box<Node>`); by-value cycles have infinite size and are rejected.

`String` (owned UTF-8), `Vec<T>`, `Box<T>`, `Option<T>`, `Result<T,E>`, `HashMap`, `HashSet`, and `Iterator<T>` are standard-library concepts, not primitive compiler types. See [type-system.md](type-system.md).

## Strings

String literals are static borrowed UTF-8 `str`. `String` is owned growable UTF-8 bytes; `str` is a borrowed string slice and is not necessarily NUL-terminated. Length is bytes; character iteration decodes Unicode scalar values. Embedded NUL is valid. Equality compares UTF-8 byte content without normalization. Formatting and concatenation belong to library APIs. The current runtime print ABI is temporarily NUL-terminated and rejects NUL-containing native literals; that implementation restriction is not the designed string model.

## References, ownership, memory

Proposed syntax: `&'a T` shared reference, `&'a mut T` exclusive mutable reference, `*const T` / `*mut T` raw pointers. `mut` is a reserved keyword restricted to these type/receiver forms. References are non-null, aligned, valid, and cannot outlive the owner. Local borrow lifetimes infer; function parameters may elide input lifetimes, but a reference result must name its source lifetime unless exactly one borrowed input (including a receiver) makes it unambiguous. Raw pointers may be null/dangling; dereference and pointer arithmetic require explicit `unsafe { ... }`, including inside unsafe functions. Pointer arithmetic must stay within an allocation; integer casts do not establish pointer validity. Converting `&T` to `*const T` and `&mut T` to `*mut T` preserves the source loan/provenance; `&T` cannot become `*mut T`. Raw-pointer writes require `*mut T` and unsafe. Recreating safe references from raw pointers is unsafe and must re-establish all validity, lifetime, and exclusivity guarantees.

Proposed model is affine ownership: one owner for each non-`Copy` value, moves on by-value transfer, explicit `Copy` only for duplicable values. By-value parameters take ownership; callers use `&`/`&mut` to borrow. Shared borrows may coexist; mutable borrow is exclusive; borrow cannot outlive owner. Struct fields may be partially moved only for types without a destructor; arrays and tuples cannot be partially moved. Drops happen exactly once at scope exit in reverse declaration/initialization order. `Box<T>` owns a heap allocation. Safe code should prevent use-after-free/double-free; intentional leaks and unsafe errors remain possible. This is not enforced today. Full obligations and limits are in [memory-model.md](memory-model.md).

## Structs, enums, methods, traits

Struct fields are private by default, individually exportable with `pub`. Construct by named fields; omitted fields are errors absent a future update syntax. Enums support unit/tuple/struct variants; payload fields inherit the enum/variant visibility and do not take separate `pub` modifiers. Patterns destructure values and exhaustiveness is mandatory. `impl Type { func method(&self, value: Type) -> Type { return value; } }` groups methods and trait implementations; receiver mode is explicit as `self`, `&self`, or `&mut self`. Associated constructors are ordinary methods without a receiver. `value.field` accesses a field; `value.method(args)` invokes a method. Parentheses select the operation; no silent field-to-method rewrite occurs.

`trait` declares required and default methods. Generic bounds use traits. Monomorphized static dispatch is selected; `dyn Trait` is explicit dynamic dispatch and deferred until object-safety and stable vtable rules are designed. Trait implementations obey an orphan/coherence rule: an implementation is allowed only when the trait or the implementing type belongs to the current package; overlapping implementations are rejected. Associated types are deferred; initial traits use generic type parameters. No methods, structs, enums, or traits compile today.

## Control flow, expressions, operators

Designed forms: `if/else`, `while`, `for item in collection`, `loop`, `break`, `continue`, `return`, and `match`. Conditions must be bool. Loops do not yield values initially. Optional loop labels use `label: while condition { ... }`, `label: for item in values { ... }`, or `label: loop { ... }`; `break label;` and `continue label;` target that loop. Unlabeled exits target the innermost loop. `match` is selected instead of `switch`, and is a statement, not a value expression. Enum and bool cases must be exhaustive; wildcard handles remaining scalar values; guarded arms do not count toward exhaustiveness. `if` and blocks are statements, not expressions. `&&`/`||` short-circuit. Evaluation is left-to-right for operands, call arguments, and field/base/index expressions. Assignment evaluates its place once before the right-hand side; compound assignment reads the old value before evaluating the right-hand side. Only a local or explicit field/tuple/index/dereference place can be assigned; raw dereference writes require unsafe and a mutable pointer. Tuple syntax distinguishes `(T)`/`(value)` grouping from `(T,)`/`(value,)` one-element tuples; empty tuple/unit syntax is not selected. Ranges are half-open for `..`, inclusive for `..=`, use same-typed integer endpoints, step by one, and descending ranges are empty. Full precedence/associativity is in [syntax-reference.md](syntax-reference.md).

| Precedence (high to low) | Operators | Associativity |
|---|---|---|
| 12 | call `()`, index `[]`, member `.`, cast `as Type`, propagation `?` | left; postfix chain |
| 11 | unary `! - ~ & &mut *` | prefix |
| 10 | `* / %` | left |
| 9 | `+ -` | left |
| 8 | `<< >>` | left |
| 7–5 | bitwise `&`, `^`, `|` | left |
| 4 | `< <= > >=` | non-associative |
| 3 | `== !=` | non-associative |
| 2–1 | `&&`, `||` | left, short circuit |
| 0 | `= += -= *= /=` | right, statement form |

Assignment is not an expression. Compound assignment has the same checked arithmetic behavior as its expanded form. String equality is content equality; pointer equality is address equality only.

## Modules and visibility

`module` declares canonical source identity; `import` imports a different module's public namespace; `pub import` re-exports that namespace. Example: `module net.http; import std.io as io; pub import network.http as http;`. An unambiguous source path may infer identity; if a declaration exists it must match. `src/main.jahe` is the conventional executable entry path, selected explicitly by the manifest's `entry`; library packages use `src/lib.jahe` and omit `entry`. `src/net/http.jahe` maps to `net.http`. `jahe.toml` configures package/dependencies; generated `jahe.lock` fixes dependency resolution. In expressions, `.` is the only member/namespace operator. See [module-system.md](module-system.md).

Imports are file-scoped and precede items. `import a.b;` binds root namespace `a`; `import a.b as c;` binds `c` directly to module `a.b`; `pub import` re-exports by the same binding rule. Paths use only dots. Conflicting aliases, cycles, and private access are errors. Wildcard imports are excluded. Visibility is private by default; `pub` exports items and fields. See [module-system.md](module-system.md).

## Errors, panic, and unsafe

Recoverable failure uses must-use library `Result<T,E>`; absence uses must-use `Option<T>`. Exhaustive `match` is the base handling mechanism. Such values must be handled, returned, propagated with `?`, or explicitly discarded as `let _ = expression;`; `_` binds no name, and there is no `discard` statement. Postfix `?` propagates `Err` only in a function returning `Result<T,E>` with the same error type, or propagates `None` only in a function returning `Option<T>`; no implicit conversion occurs. Optional chaining such as `value?.field` is not selected. Exceptions/`throw` are not selected. `panic(message)` is unrecoverable and aborts without unwinding or running destructors. No panic unwinds across a native ABI. Runtime services return status/null at the C boundary; wrappers map those to `Result` before source code. See [error-handling.md](error-handling.md).

`unsafe` is the official boundary keyword, but implementation is deferred. `unsafe func` makes callers responsible for documented preconditions; unsafe operations within its body still require an explicit unsafe block. It covers raw dereference/arithmetic, FFI calls, union access, mutable static access, volatile/assembly and manual allocator contracts. Unsafe does not suspend type checking or make invalid references valid. Inline assembly, atomics, volatile, packed layout and conditional compilation are future designs.

## Generics and callable values

`func identity<T>(value: T) -> T` is proposed generic syntax; bounds are `T: Constraint + Other`. Generic functions/types use monomorphization; type inference may infer arguments only when unique. No specialization. Function values spell `func(A, B) -> R`; nullable function pointers use `Option<func(i32) -> i32>`. Closures are deferred from the initial core; if introduced, captures must be listed explicitly (`|capture x| ...`) and default to move, with explicit shared/mutable borrow capture forms. This avoids hidden lifetime extension. Async/await, variadic JAHE functions and arbitrary compile-time metaprogramming are deferred.

## Standard library/platform boundary

Runtime is process-level ABI support; compiler built-ins are limited to irreducible language/target operations; standard library supplies ownership wrappers, strings, collections, I/O and platform APIs; third-party packages provide HTTP/image/audio/JSON. Proposed namespaces include `std.io`, `std.fs`, `std.net`, `std.time`, `std.collections`, `std.string`, `std.math`, `std.thread`, and `std.sync`; no such JAHE modules are currently implemented. Full boundary is in [standard-library-design.md](standard-library-design.md).

Compiler 0.7 native support is Windows x86_64/MSVC only. Linux, macOS, ARM, cross compilation, stable aggregate ABI, source FFI and non-Windows calling conventions are not implemented. Target selection must drive data layout; host layout must not be used accidentally for a cross target.

Concurrency is deferred but has a selected safety direction: ownership transfer for thread inputs, auto `Send`/`Sync` marker traits for cross-thread values, `Mutex`/channels for synchronization, and atomics with `Relaxed`, `Acquire`, `Release`, `AcqRel`, or `SeqCst`. `Consume` is omitted. Compiler-derived marker implementations are checked structurally; manual implementations require `unsafe impl` of an `unsafe trait`. Data races are undefined behavior and must be prevented in safe code; volatile is unsafe device/interoperability access, not synchronization. Future `std.thread.spawn(func() -> T) -> Result<JoinHandle<T>, ThreadError>` transfers `Send` input; `join` consumes the handle and returns `Result<T, ThreadError>`. Thread panic aborts the process, so join errors report OS/runtime failure rather than a panic payload. Async/await are not reserved keywords and have no source syntax/runtime support.

Compile-time evaluation is limited in the initial design to literal/constant expressions and pure `const func` calls. A `const func` may use local immutable bindings, conditionals, loops with statically bounded evaluation budget, and calls to other const functions; it cannot perform I/O, allocation, mutation, FFI, or target-dependent host queries. Evaluation is deterministic for a selected target and bounded by compiler resource limits; exceeding a limit is a diagnostic. Conditional compilation, macros, and arbitrary metaprogramming are deferred.

## Deferred extensions

Deferred features are explicitly not part of current compiler support: structs/enums/tuples and match, generics/traits/methods, ownership/borrowing/destructors, raw pointers/unsafe, imports/re-exports/package management, source FFI, closures, async, unions, block/if/match expressions, static variables, inline assembly, atomics/threading, conditional compilation, stable aggregate ABI, and non-Windows targets.

No core semantic blocker remains for this proposed design. Deferred subsystem details are intentionally scoped out: registry trust-key rotation/mirrors, workspaces/features, multiple package versions in one graph, dynamic trait-object ABI, closure environment ABI, `repr(C)` aggregate details, and additional platform ABIs. These are extension decisions, not reasons to leave the core rules vague. Review [language-design-decisions.md](language-design-decisions.md) and [specification-finalization-report.md](specification-finalization-report.md). This document is not frozen; user approval is required before calling it JAHE Language Specification v1.0.

## Complete designed example (not executable today)

```jahe
module app;
import std.io as io;
import std.string as strings;

struct User { id: u64, name: strings.String, active: bool }
enum Status { Pending, Active, Disabled }
func add(a: i32, b: i32) -> i32 { return a + b; }

func main() -> i32 {
    let user: User = User { id: 1, name: strings.String.from("JAHE"), active: true };
    var count: i32 = 10;
    if count > 0 { io.println("JAHE"); }
    count = add(count, 1);
    return 0;
}
```

This is designed syntax only: `import`, struct values, `String`, and `func` are not accepted by compiler 0.7. See [representative examples](examples/).

## Implementation order

See [language-roadmap.md](language-roadmap.md). In brief: primitive/conversion semantics; aggregates/layout/patterns; ownership/borrowing/unsafe; generics, traits, `Option`/`Result`; methods/callable values; modules/package graph; standard library and more targets. Existing compiler behavior must be preserved and each syntax change tested; this documentation milestone changes no parser or backend.
