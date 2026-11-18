# JAHE language specification (design baseline)

**Status:** design milestone. This document records proposed architecture; it does not imply implementation. “Implemented” means accepted by this repository's compiler today. “Official design” means selected direction but possibly not implemented. “Deferred” is intentionally outside the initial language core. “Open” requires a later decision.

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

See [language-spec.md](language-spec.md) for accepted forms, [syntax-reference.md](syntax-reference.md) for the designed surface, and [backend-design.md](backend-design.md)/[runtime-abi.md](runtime-abi.md) for verified implementation facts.

## Keywords, declarations, and scope

Official design keywords are `func`, `return`, `let`, `var`, `const`, `module`, `import`, `as`, `pub`, `if`, `else`, `while`, `for`, `in`, `loop`, `break`, `continue`, `match`, `struct`, `enum`, `impl`, `trait`, `unsafe`, `extern`, `true`, and `false`. `self` is a reserved method receiver. `async` and `await` are reserved candidates, not initial syntax. Keyword policy and rationale are in [language-design-decisions.md](language-design-decisions.md).

A function declaration has the form `func name(parameters) -> Type { return value; }`; omitted result type means `void`. Parameters are typed. `return;` is only for void functions; `return value;` must match the result. One function namespace per module; no overloading. Direct recursion is allowed; module-level signatures are collected before checking bodies to permit mutual recursion. Functions are private by default; `pub` exports them. Ordinary calls use a compiler-controlled target ABI, not a stable FFI ABI. Variadic functions are deferred. Foreign declarations use `extern "C" func` and require an unsafe call boundary; exact C mappings remain open.

`let name: T = value;` binds an immutable local; `var name: T = value;` binds a mutable local; `const NAME: T = value;` declares a compile-time constant. Initializers are required. Local annotations may be omitted only when the initializer determines a unique type. No implicit lossy conversions. Lexical blocks scope names; nested shadowing is allowed, same-scope duplicates are errors. Destructuring, mutable global storage and `static` are deferred. A future `static` needs explicit initialization and concurrency rules.

The native entry is designed as `pub func main() -> i32` in the chosen package root. A future manifest selects that root. Compiler 0.7 uses existing `fn` syntax and has its own single-file CLI conventions.

## Type system

Proposed primitives: `bool`; signed `i8/i16/i32/i64/i128/isize`; unsigned `u8/u16/u32/u64/u128/usize`; `f32/f64`; `char`; `void`; `never`. Fixed integer widths are target-independent; `isize`/`usize` match target pointer width. Alignment and aggregate layout follow the selected target ABI. `char` is a Unicode scalar, not a byte. `void` is absence of a result; `never` has no values and can inhabit a result position because it never returns.

Integer literals infer from context or default to the smallest fitting signed `i32`, `i64`, `i128`; otherwise require an explicit fitting unsigned type or error. Floats default to `f64`. Integer arithmetic is checked in the design baseline; division by zero and signed min divided by -1 fail. Wrapping must be explicit. Floating arithmetic follows IEEE 754. `as` is explicit; narrowing and float-to-integer conversion are checked (exact recoverable error syntax is open). No implicit narrowing or signedness conversion.

Arrays `[T; N]` are fixed-size inline sequences; slices `&[T]` are pointer-plus-length views; tuples are positional products; structs are nominal records; enums are tagged sums with unit, tuple, or named-field variants. Safe indexing is bounds-checked. Enum matching is exhaustive unless wildcard covers remaining cases. Union layout is deferred; reading a non-active member would require unsafe. Recursive aggregates require indirection (`Box<Node>`); by-value cycles have infinite size and are rejected.

`String` (owned UTF-8), `Vec<T>`, `Box<T>`, `Option<T>`, `Result<T,E>`, `HashMap`, `HashSet`, and `Iterator<T>` are standard-library concepts, not primitive compiler types. See [type-system.md](type-system.md).

## Strings

String literals are static borrowed UTF-8 `str`. `String` is owned growable UTF-8 bytes; `str` is a borrowed string slice and is not necessarily NUL-terminated. Length is bytes; character iteration decodes Unicode scalar values. Embedded NUL is valid. Equality compares UTF-8 byte content without normalization. Formatting and concatenation belong to library APIs. The current runtime print ABI is temporarily NUL-terminated and rejects NUL-containing native literals; that implementation restriction is not the designed string model.

## References, ownership, memory

Proposed syntax: `&T` shared reference, `&mut T` exclusive mutable reference, `*const T` / `*mut T` raw pointers. References are non-null, aligned, valid, and cannot outlive the owner. Raw pointers may be null/dangling; dereference and pointer arithmetic require `unsafe`. Pointer arithmetic must stay within an allocation; integer casts do not establish pointer validity.

Proposed model is affine ownership: one owner for each non-`Copy` value, moves on by-value transfer, explicit `Copy` only for duplicable values. Shared borrows may coexist; mutable borrow is exclusive; borrow cannot outlive owner. Drops happen exactly once at scope exit in reverse initialization order. `Box<T>` owns a heap allocation. Safe code should prevent use-after-free/double-free; intentional leaks and unsafe errors remain possible. This is not enforced today. Full obligations and limitations are in [memory-model.md](memory-model.md).

## Structs, enums, methods, traits

Struct fields are private by default, individually exportable with `pub`. Construct by named fields; omitted fields are errors absent a future update syntax. Enums support unit/tuple/struct variants. Patterns destructure values and exhaustiveness is mandatory. `impl Type { func method(&self, value: Type) -> Type { return value; } }` groups methods and trait implementations; receiver mode is explicit as `self`, `&self`, or `&mut self`. Associated constructors are ordinary methods without a receiver.

`trait` declares required and default methods. Generic bounds use traits. Monomorphized static dispatch is initial direction; `dyn Trait` is explicit dynamic dispatch with a vtable, subject to object-safety rules. Associated types and coherence detail are deferred. No methods, structs, enums, or traits compile today.

## Control flow, expressions, operators

Designed forms: `if/else`, `while`, `for item in collection`, `loop`, `break`, `continue`, `return`, and `match`. Loops do not yield values initially. `match` is selected instead of a separate `switch`. Whether `if` becomes an expression is open; initial design treats it as a statement. `&&`/`||` short-circuit. Full precedence/associativity and assignment policy are in [syntax-reference.md](syntax-reference.md).

| Precedence (high to low) | Operators | Associativity |
|---|---|---|
| 12 | call, index, member, cast `as Type`, propagation `?` | left |
| 11 | unary `! - ~ & &mut *` | prefix |
| 10 | `* / %` | left |
| 9 | `+ -` | left |
| 8 | `<< >>` | left |
| 7–5 | bitwise `&`, `^`, `|` | left |
| 4 | `< <= > >=` | non-associative |
| 3 | `== !=` | non-associative |
| 2–1 | `&&`, `||` | left, short circuit |
| 0 | `= += -= *= /=` | right, statement form |

Ranges `..` and `..=` are candidates for iteration/slicing; exact endpoints are open. Assignment is not an expression. Compound assignment has the same checked arithmetic behavior as its expanded form. String equality is content equality; pointer equality is address equality only.

## Modules and visibility

`module` declares canonical source identity; `import` makes a different module's public namespace available. Example: `module net.http; import std.io as io;`. An unambiguous source path may infer identity; if a declaration exists it must match. Proposed package layout maps `src/main.jahe` to package root and `src/net/http.jahe` to `net.http`. `jahe.toml` describes package root, edition, target and dependencies; its schema/lockfile are open. No manifest is needed for current one-file CLI.

Imports are file-scoped and precede items. `import a.b;` binds `b`; `import a.b as c;` binds `c`; qualified item names use `b::Name` / `c::Name`. No wildcard imports or implicit re-export. Conflicting aliases and private access are errors. Signature collection precedes body checking, allowing declaration cycles; dynamic global initializers are excluded. Visibility is private by default; `pub` exports items and fields. See [module-system.md](module-system.md).

## Errors, panic, and unsafe

Recoverable failure uses library `Result<T,E>`; absence uses `Option<T>`. Exhaustive `match` is the base handling mechanism. Postfix `?` is a proposed later equivalent of explicit error propagation, pending conversion/coherence rules; it is not optional member access. Optional chaining such as `value?.field` is not selected for the initial core. Exceptions/`throw` are not selected. `panic(message)` is an unrecoverable `never` operation and currently aborts; it is not for normal I/O errors. See [error-handling.md](error-handling.md).

`unsafe` is the official boundary keyword, but implementation is deferred. It will mark raw dereference/arithmetic, FFI calls, union access, mutable static access, volatile/assembly and manual allocator contracts. Unsafe does not suspend type checking or make invalid references valid. Inline assembly, atomics, volatile, packed layout and conditional compilation are future designs.

## Generics and callable values

`func identity<T>(value: T) -> T` is proposed generic syntax; bounds are `T: Constraint + Other`. Generic functions/types use monomorphization first; type inference may infer arguments when unique. Compile-time evaluation should begin with constants and pure `const func`, not arbitrary macros. Function values spell `func(A, B) -> R`; a nullable code pointer may be `Option<func(i32) -> i32>`. Anonymous functions, closures, capture modes and lifetimes are deferred. Higher-order functions follow once function values exist. `async`/`await`, variadic JAHE functions, specialization and arbitrary compile-time metaprogramming are deferred.

## Standard library/platform boundary

Runtime is process-level ABI support; compiler built-ins are limited to irreducible language/target operations; standard library supplies ownership wrappers, strings, collections, I/O and platform APIs; third-party packages provide HTTP/image/audio/JSON. Proposed namespaces include `std.io`, `std.fs`, `std.net`, `std.time`, `std.collections`, `std.string`, `std.math`, `std.thread`, and `std.sync`; no such JAHE modules are currently implemented. Full boundary is in [standard-library-design.md](standard-library-design.md).

Compiler 0.7 native support is Windows x86_64/MSVC only. Linux, macOS, ARM, cross compilation, stable aggregate ABI, source FFI and non-Windows calling conventions are not implemented. Target selection must drive data layout; host layout must not be used accidentally for a cross target.

## Deferred and unresolved

Deferred: aggregate syntax/type-check/codegen, patterns, generics, methods/traits, ownership/borrow checking, destructors, raw pointers, unsafe, import/package resolution, FFI, closures, async, inline assembly, atomics, conditional compilation and standard library modules.

Open: conversion failure API; checked arithmetic finalization; stable `repr(C)` and enum layout; `?` conversion/coherence; nested visibility/re-export; closure capture defaults; `if` expression semantics; range endpoints; atomics memory ordering; package registry/lock format; compile-time evaluation limits; public generic ABI. Decisions and alternatives are tracked in [language-design-decisions.md](language-design-decisions.md).

## Complete designed example (not executable today)

```jahe
module app;
import std.io as io;
import std.string as strings;

struct User { id: u64, name: strings::String, active: bool }
enum Status { Pending, Active, Disabled }
func add(a: i32, b: i32) -> i32 { return a + b; }

func main() -> i32 {
    let user: User = User { id: 1, name: strings::String::from("JAHE"), active: true };
    var count: i32 = 10;
    if count > 0 { io::println("JAHE"); }
    count = add(count, 1);
    return 0;
}
```

This is designed syntax only: `import`, struct values, `String`, and `func` are not accepted by compiler 0.7. See [representative examples](examples/).

## Implementation order

See [language-roadmap.md](language-roadmap.md). In brief: primitive/conversion semantics; aggregates/layout/patterns; ownership/borrowing/unsafe; generics, traits, `Option`/`Result`; methods/callable values; modules/package graph; standard library and more targets. Existing compiler behavior must be preserved and each syntax change tested; this documentation milestone changes no parser or backend.
