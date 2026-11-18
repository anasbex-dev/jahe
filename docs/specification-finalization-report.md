# JAHE specification finalization report

**Audit outcome: READY FOR USER REVIEW.** The design is proposed and is not frozen as JAHE Language Specification v1.0. Explicit user approval is still required for a v1.0 freeze. This was a documentation-only audit; it did not validate compiler implementation of designed features.

## Decisions recorded

The four approved decisions are recorded in [language-design-decisions.md](language-design-decisions.md) and specified in the documents listed here:

| Decision | Normative locations | Result |
|---|---|---|
| Field/method syntax | [language-specification.md](language-specification.md), [module-system.md](module-system.md), [syntax-reference.md](syntax-reference.md) | `x.name` is a field/member; `x.name(args)` requests a method. Generic member/call AST nodes defer classification to resolution. Same-name fields and methods are selected by punctuation; callable fields use `(x.callback)()`. No implicit rewrite. |
| Tuple syntax/access | [grammar.ebnf](grammar.ebnf), [type-system.md](type-system.md), [syntax-reference.md](syntax-reference.md) | `(x,)` is a singleton tuple and `(x)` grouping. `tuple.0` is a statically range-checked positional projection, lexically distinct from an identifier-named struct field. |
| Module re-export | [module-system.md](module-system.md), [language-specification.md](language-specification.md) | `pub import` uses the same alias/root binding rules as import; it exposes a reachable public namespace only. Every segment and re-export boundary is checked. Import and re-export cycles are diagnosed and rejected. |
| Package manifest and lock | [module-system.md](module-system.md), [language-design-decisions.md](language-design-decisions.md) | `jahe.toml` declares package identity, edition, source/entry and path/Git/registry requirements. `jahe.lock` records deterministic selected graph, sources, hashes and tool/runtime/target compatibility inputs. Conflicts fail; locked builds do not silently resolve. |

## Other design closure

The audited documents define name/value/type lookup order, import aliases and lexical shadowing, visibility at module and API boundaries, literal inference and checked arithmetic/conversions, `void`/`never`, private aggregate layout, generic constraints and monomorphization, function pointer and deferred closure behavior, trait coherence, ownership/borrowing/lifetimes, partial moves and drop order, unsafe operations, control-flow and evaluation order, exact `Option`/`Result` propagation, panic-abort behavior, runtime/FFI limits, concurrency direction, and compiler/runtime/library boundaries. Diagnostics and implementation prerequisites are included where these rules require validation.

The chosen trade-offs and rejected alternatives are summarized in [language-design-decisions.md](language-design-decisions.md): explicit imports over wildcard/implicit names, punctuation-selected fields/methods over silent rewriting, static tuple projection over dynamic indexing syntax, value-based errors over exceptions and conversion-driven propagation, affine ownership over manual-only memory or mandatory tracing collection, private target layout over implicit C layout, and an immutable lock graph over floating package resolution.

## Remaining issues and deferred features

No blocking decision remains for the proposed initial language design. The following are extension work, explicitly outside the initial contract or requiring later prerequisites: unions and `repr(C)` aggregates; associated types, specialization and dynamic trait objects; closures and async; block, `if`, and `match` expressions; globals/statics; inline assembly, volatile access, atomics and threads; conditional compilation; workspaces/features, registry trust-key lifecycle and multiple versions of one package in a dependency graph; and non-Windows ABI support. The registry trust protocol and signing/key-rotation policy need a separate security design before registry implementation. No safety claim is permitted until ownership and borrow analysis is implemented and validated.

## Consistency and validation

The cross-document audit covered the main specification, syntax reference, EBNF, decisions, type system, memory model, modules/packages, error handling, standard-library design, language roadmap, compiler roadmap, README, and all files in `docs/examples/`.

- The four approved decisions use consistent syntax and behavior across the specification, grammar, examples and roadmap.
- `.` is the only designed source operator for namespace, submodule, member, method, associated-item and tuple access. `::` appears only as an explicit statement that it is not JAHE syntax (and as Rust implementation notation where applicable).
- Field/method examples agree on punctuation-based resolution and callable-field grouping.
- Tuple grouping, singleton tuples and `.N` projections agree across grammar, prose and examples.
- Import/re-export examples distinguish exact aliases from root namespace bindings; private declarations are not made public by re-export.
- Manifest and lockfile names and responsibilities are consistent. Reproducibility claims are limited: a lock pins dependency inputs and recorded compatibility settings, while executable-byte identity also depends on toolchain/linker/build determinism.
- Designed and deferred features are not described as compiler-0.7 support. The implementation snapshot remains separate from the proposed design.
- Relative Markdown links and referenced example paths were checked after the report was added; EBNF and source examples were reviewed for agreement.
- No compiler tests were run: implementation was out of scope, and this report makes no implementation-validation claim.

## Dependencies and suggested implementation order

The dependency sequence is recorded in [language-roadmap.md](language-roadmap.md):

1. Core syntax, scalar types, literal inference, checked arithmetic/conversions, and generic member AST/name-resolution foundations.
2. Aggregate layout, tuple projection, enums, patterns and statement-form `match`.
3. Ownership, borrow/lifetime analysis, drops and unsafe boundaries; gate all safety claims on soundness testing.
4. Generic instantiation, trait constraints/coherence, function pointers, `Option`/`Result` and exact `?` propagation.
5. Methods and callable values, followed later by closures only after capture and lifetime rules are established.
6. Multi-module resolution, visibility, `pub import`, cycle diagnostics, then manifest parsing and deterministic locked dependency resolution.
7. Standard-library growth and platform ABI expansion, each backed by runtime compatibility and target-specific tests.

This order is a dependency recommendation, not a release promise. The proposed specification is ready for review; it must not be called frozen until the user explicitly approves v1.0.
