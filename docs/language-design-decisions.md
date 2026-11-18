# JAHE language design decisions and status

This records the current proposed contract for review. It is not a v1.0 freeze. Current compiler facts are in [language-spec.md](language-spec.md); the full design is in [language-specification.md](language-specification.md).

## Status labels

- **Implemented:** accepted and supported by the compiler in this repository (currently the limited compiler-0.7 snapshot only).
- **Designed:** syntax/semantics are described here but compiler support is absent.
- **Deferred:** intentionally excluded until listed prerequisites are met.
- **Experimental:** unstable behavior that may change before v1.0. No source-language feature is currently designated experimental; compiler internals/ABI details are not a stable language contract.

## Approved core decisions

| Feature | Decision | Alternative rejected / reason | Status |
|---|---|---|---|
| Function | `func` | `fn` retained only as current legacy compiler syntax; `func` is the JAHE spelling | Designed |
| Bindings | immutable `let`, mutable `var`, `const` compile-time | `let mut` is current syntax only; explicit mutability at declaration is clearer | Designed |
| Mutability marker | `mut` is a reserved keyword restricted to reference/pointer/receiver forms | Contextual keywording complicates lexing and creates inconsistent identifiers | Designed |
| Method receiver | reserved `self` value only in a method with a declared receiver; associated functions have no `self` | Implicit receiver in static methods would create hidden ownership | Designed |
| Executable entry | manifest `entry` selects a private ordinary `func main() -> i32`, no parameters/generics; libraries omit entry | Public visibility is unrelated to startup; argument-vector and void returns add runtime/ABI cases before needed | Designed |
| Identity/import | `module` declares identity; `import` imports a dependency; `pub import` re-exports | Combining them would confuse identity with visibility | Designed |
| Access operator | `.` for namespace, submodule, field, method, associated item, tuple index | No `::`; one chain AST leaves meaning to semantic analysis | Designed |
| Field/method | `x.name` is field; `x.name(args)` is method call | No silent rewrite of field access into method invocation | Designed |
| Same-name field/method | Parentheses choose method, no parentheses choose field; callable field call uses `(x.callback)()` | Avoids resolution depending on accidental callable field type | Designed |
| Tuple projection | `tuple.0` etc.; integer position statically range-checked | Bracket form rejected because it suggests dynamic array indexing | Designed |
| Tuple arity one | `(value,)` tuple, `(value)` grouping | Prevents grouping/tuple ambiguity; empty unit tuple omitted | Designed |
| Re-export | `pub import path [as alias];`; visibility checked at every path boundary | Re-export cannot make private items public | Designed |
| Import cycles | Reject all import/re-export cycles with a source-edge cycle diagnostic | Allowing declaration cycles complicates interfaces and initialization ordering | Designed |
| Manifest/lock | human `jahe.toml`; generated deterministic `jahe.lock` | One file cannot provide both author intent and resolved reproducibility | Designed |
| Numeric overflow | checked in all build modes; wrapping only explicit API | Debug/release differences rejected despite runtime-check cost | Designed |
| Conversion | explicit checked `as`; panic on dynamic failure; contextual `try_as(value)` returns `Result` | Implicit lossy cast rejected; one spelling cannot hide failure | Designed |
| Error flow | `Option`/`Result`, `match`, exact-type `?`; no exception/throw | Avoids hidden unwind and implicit error conversions | Designed |
| Explicit discard/token | Lone `_` is a reserved wildcard/discard token, never an identifier; `let _ = expression;` is the sole explicit discard form and `_` is also a wildcard pattern | A second `discard` statement or treating lone `_` as a name adds ambiguity | Designed |
| Memory | affine ownership + borrow checker + deterministic drop, no mandatory GC | Manual-only is error-prone; tracing GC has runtime/latency cost | Designed, not a safety claim |
| Match/control | `match` statement only; bool/enums exhaustive; scalar matches need wildcard | Match expressions, block expressions and if expressions deferred to keep initial CFG simple | Designed |
| Modules/packages | path modules; root binding for unaliased import; exact target binding for alias | No wildcard imports; no ambiguous bare import behavior | Designed |

## Name, type, and visibility rules

Type names and value/module names are separate namespaces. Type positions resolve types. Expression lookup checks innermost lexical values, module-level values/functions, and module aliases according to scope rules; an inner local may shadow an import, while same-scope conflicts are errors. If no value/module root resolves, a type name can start associated-item lookup. Qualify a type through its module to avoid a value shadow. Dotted paths rooted at modules resolve the longest existing module prefix. Member classification then uses symbol tables and expression types.

Items are private by default. `pub` exposes an item from its defining module; modules become externally nameable through `pub import` in their owner/root. Every path segment/re-export boundary is checked. Struct fields are separately public/private; enum variants inherit enum visibility; trait members follow the trait's visibility. Public signatures cannot expose unnameable private types. Re-export exposes a namespace path, not private declarations. See [module-system.md](module-system.md).

## Type, generic, and callable rules

Primitive widths, inference boundaries, conversion diagnostics, aggregate layout, generics, function pointers and closure direction are specified in [type-system.md](type-system.md). Summary: local inference only from unique initializer; no implicit conversion; exact checked casts; monomorphize generics; no specialization; orphan/coherence rule for traits; no associated types initially. Function pointers do not capture. Closures are deferred; future captures must be explicit, default move, with explicit borrow mode. Trait objects/dynamic dispatch and const generics are deferred.

Struct default layout preserves field order but is compiler-private; enum tags/padding are unspecified. No aggregate crosses FFI before a future stable representation annotation. C ABI is scalar/pointer-only; bool maps through `u8`, strings are pointer/length arguments, ownership does not cross implicitly.

## Memory, concurrency, and errors

Lifetime elision is permitted only for input-only references or a unique borrow source for a reference result; otherwise named `'a` lifetimes are required. Struct partial moves are allowed only for no-destructor types; tuple/array partial moves are rejected. Drop is reverse initialization/declaration order and only `impl Drop for T` may destruct. Panic aborts with no cleanup/unwind. Interior mutability is explicit through `Cell`/`Mutex`; `Send`/`Sync` future markers enforce cross-thread safety. Atomic orderings are `Relaxed`, `Acquire`, `Release`, `AcqRel`, `SeqCst`; volatile is not synchronization and is unsafe. Threading/sync/atomics are deferred until borrow safety and runtime support.

`?` only propagates same-error `Result` or `Option` within a matching return type. Error conversions are explicit. C ABI never carries JAHE enums/results and never permits unwinding across boundary. Full rules in [memory-model.md](memory-model.md), [error-handling.md](error-handling.md), and [runtime-abi.md](runtime-abi.md).

Reborrows suspend their mutable parent until child last use; distinct declared fields can be borrowed independently, while dynamic indices conservatively overlap. Loan state is tracked across control flow and merged conservatively. Temporaries last through their full statement, with direct local-reference initialization extending the borrowed temporary to the local scope. Raw pointers retain provenance/loans; raw dereference and reconstitution of safe references require unsafe and cannot bypass reference validity. Assignment uses only explicit places; destination evaluation occurs once before the RHS, while compound assignment reads the destination before evaluating the RHS. See [memory-model.md](memory-model.md).

`mut` is reserved and allowed only in the documented mutable reference, raw-pointer, and receiver forms. `self` is an ordinary receiver value only in a method declaring `self`, `&self`, or `&mut self`; its move/borrow/mutation behavior is exactly that receiver mode. Associated functions cannot use `self`.

`Result` and `Option` are must-use. They must be consumed, returned, propagated, or explicitly discarded with `let _ = expression;`. `_` never binds a local name; in patterns it is a wildcard. No `discard` keyword is introduced. Unused ordinary non-void expression results warn; void expression statements are valid.

## Manifest and reproducibility decision

`jahe.toml` declares package name, SemVer package version, language edition, source root, optional executable entry, and path/Git/registry dependencies. `jahe.lock` pins resolved versions, canonical sources, Git commit IDs, SHA-256 package/tree hashes, root manifest hash, compiler version, target/configuration, and sorted dependency edges. Resolution uses intersected SemVer requirements, one selected source/version per package name in the graph, and errors on conflicts. Git requires immutable full commit; registry downloads use HTTPS and are checked against locked registry metadata hashes, but this does not authenticate publishers. Path changes invalidate lock hashes. `jahe update` changes the lock; locked builds never resolve new versions. Registry trust/key rotation, workspaces, optional features, target-specific dependency expressions, and multiple simultaneous versions are deferred. Exact manifest examples and behavior are in [module-system.md](module-system.md).

An HTTPS connection protects transport to the configured registry endpoint. SHA-256 in the lock detects content differing from the expected locked digest but cannot establish who published it or whether that publisher is trustworthy. A lockfile makes a previously resolved graph reproducible only relative to its recorded sources and hashes; it does not authenticate a first resolution. Source identity is part of the lock and same-name dependencies from different sources conflict, preventing silent source substitution during locked builds, but a malicious registry/index, compromised first resolution, or malicious package remains possible. Registry signatures, trust roots, publisher identity/authorization and key rotation are deferred; these require a separately specified trust and recovery policy.

The package `entry` field selects the executable source containing one private, non-generic `func main() -> i32` with no parameters. `main` is a normal function that the build system recognizes only at the designated executable entry. Its result maps to the platform process exit status. Libraries omit `entry` and have no required process entry; any library-local `main` is ordinary and not selected. See [module-system.md](module-system.md) and [language-specification.md](language-specification.md).

## Alternatives evaluated and recommendation

- **Ownership:** manual allocation gives control but increases ordinary bug burden; tracing collection simplifies reclamation but adds runtime/scheduling costs. Recommend affine ownership and borrows, with explicit unsafe; prerequisite: sound checker before claiming safety.
- **Modules:** implicit globals and wildcard imports are concise but create name collisions. Recommend explicit imports, root/alias rules, private defaults, and no wildcards.
- **Error handling:** exceptions add hidden control flow; conversion-heavy `?` creates coherence complexity. Recommend `Result`/`Option`, exhaustive match, and exact-type propagation.
- **Layouts:** implicit C-compatible layout blocks optimization and is platform brittle. Recommend private native layout and explicit future ABI representations; FFI currently scalars/pointers only.
- **Closures:** implicit capture is convenient but hides ownership/lifetime effects. Recommend defer; future explicit captures with move default.
- **Packages:** floating versions simplify updates but damage reproducibility. Recommend semver ranges in manifest plus committed immutable lock graph and content hashes.

## Deferred rather than unresolved

The following are explicitly deferred, not blocking decisions for the initial language design: unions, `repr(C)` aggregates, associated types, specialization, dynamic trait objects, closures/async, block and match expressions, `if` expressions, globals/statics, manual allocators in safe code, threads/atomics/volatile, inline assembly, conditional compilation, package workspaces/features, registry trust-key lifecycle, multiple versions of a package, and non-Windows target ABIs. Each requires its listed prerequisites before implementation; no release date is promised.

The design is ready for user review once the accompanying finalization report's validation passes. It must not be labeled frozen until the user explicitly approves v1.0.
