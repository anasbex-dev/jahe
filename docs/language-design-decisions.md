# JAHE language design decisions

This records selected language direction, not compiler support. “Official design” is the current baseline for future implementation; “deferred” is deliberately not promised; “open” requires a later decision. Revisions should update this file and [the specification](language-specification.md) together.

## Selected baseline

| Topic | Decision | Reason / trade-off | State |
|---|---|---|---|
| Function declaration | `func` | Distinct visual identity; clear declaration word | Official design; compiler still uses `fn` |
| Local immutable/mutable | `let` / `var` | Mutability visible at binding site, without `let mut` modifier | Official design; compiler uses `let` / `let mut` |
| Compile-time constant | `const` | Separate from storage-backed mutable globals | Official design; not implemented |
| Source identity/use | `module` / `import` | Identity and dependency use are different responsibilities | Official design; compiler supports only module identity |
| Visibility | private by default, `pub` opt-in | Reduces accidental API surface; avoids redundant `private` keyword | Official design; source visibility not implemented |
| Types | `iN/uN`, `isize/usize`, `f32/f64`, `bool`, `char`, `void`, `never` | Width is visible; pointer-sized types are clearly target-dependent | Proposed type family; only current subset implemented |
| Integer overflow | Checked arithmetic in every build; wrapping must be explicit | Avoids debug/release semantic drift; has runtime-check cost | Selected design baseline; cast failure details remain open |
| Strings | `String` owned, `str` borrowed, literal static borrowed UTF-8 | Separates ownership and view semantics; no ambiguous primitive `string` | Official design; compiler has `str` only |
| Sum branching | `match` | Supports enum exhaustiveness and destructuring, unlike integer-only switch | Official design; deferred |
| Methods | `impl` | Groups inherent methods and trait implementations consistently | Official design; deferred |
| Interfaces | `trait` | Supports constraints and static/dynamic dispatch vocabulary | Official design; deferred |
| Memory safety | Affine ownership + exclusive mutable borrowing | Allows deterministic destruction without GC while retaining systems control | Proposed architecture; not a guarantee today |
| Error handling | `Result`/`Option`, explicit matching, later `?`; no exceptions | Makes recoverable failure visible in function types | Official direction; library types not yet implemented |
| Native FFI | Explicit `extern "C"`, unsafe call boundary | Avoids accidentally treating compiler ABI as stable | Design; source FFI deferred |
| Module layout | `jahe.toml`, `src/path/name.jahe` | Conventional mapping, one package root, no required module line per file | Proposed; package format unresolved |
| Generics | `<T: Constraint>`, monomorphized first | Predictable native code and no mandatory runtime dispatch | Proposed; deferred |

## Keywords

**Official design keywords:** `func`, `return`, `let`, `var`, `const`, `module`, `import`, `as`, `pub`, `if`, `else`, `while`, `for`, `in`, `loop`, `break`, `continue`, `match`, `struct`, `enum`, `impl`, `trait`, `unsafe`, `extern`, `true`, `false`. `self` is reserved as the explicit method receiver. `async` and `await` are reserved candidates, not enabled syntax. The current lexer accepts a smaller/different set; consult the implementation snapshot for executable source syntax.

`private` is rejected as redundant: omission means private. `switch` is not needed alongside `match`. `throw` is rejected because recoverable errors are values and panic is process termination. `try` is not selected; postfix `?` is the candidate propagation operator, subject to a later conversion/coherence decision. `mut` is not a binding keyword; `var` makes mutability explicit. `fn` remains a legacy compiler spelling only until a future migration; it is not the proposed canonical syntax.

## Explicit trade-offs

- **Checked arithmetic:** safer and deterministic, but can add runtime checks. Wrapping must be explicit. The selected baseline checks in all build modes to avoid debug/release semantic drift.
- **Private-by-default:** safer API evolution; users write `pub` for exports. It requires a deliberate re-export model later.
- **No overloads:** simpler name resolution and diagnostics. Generics handle parametric behavior; distinct names handle distinct semantics.
- **No exceptions:** clear no-hidden-control-flow model and simpler native ABI. `Result` can be verbose; `?` may reduce propagation boilerplate after generic constraints are defined.
- **Affine ownership:** avoids mandatory GC and enables deterministic cleanup, but makes compiler analysis substantially harder. No safety claim is valid before the checker is sound and tested.
- **Static dispatch first:** monomorphization simplifies runtime representation, at the cost of code size and longer builds. `dyn Trait` is explicit and later.
- **No NUL-terminated language strings:** permits embedded NUL and length-based UTF-8 views; this differs from the runtime's current temporary print ABI.
- **Module identity optional when path is unambiguous:** avoids repetitive headers, while explicit declarations can catch accidental path/package mismatch.

## Open decisions

The following are intentionally unresolved: checked conversion syntax/error type; struct packing and stable `repr(C)` guarantee; enum discriminant/layout customization; associated type and trait coherence details; exact `?` conversion behavior; closure capture defaults; `if` as expression; range endpoint semantics; atomic memory-order model; build configuration and conditional compilation; dependency registry and lockfile format; async executor model. No document should silently assume answers.

## Implemented syntax compatibility

At this revision the compiler accepts `fn`, `let` with optional `mut`, and one `module` declaration. It does not parse `func`, `var`, `const`, `import`, `pub`, types beyond the subset described in [language-spec.md](language-spec.md), or most keywords above. The proposed syntax must be introduced in later implementation milestones without changing current behavior accidentally.
