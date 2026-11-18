# JAHE language implementation roadmap

This is a proposed order, not a schedule or a promise that a feature is ready to implement. The present task completes specification design for review; it does not change compiler behavior. No phase may claim a feature implemented until parser, semantic analysis, IR/backend support where relevant, diagnostics and tests are completed.

| Phase | Scope | Prerequisites / exit criteria |
|---|---|---|
| Specification closure (current) | Define language syntax/semantics; record remaining extension topics and the status distinction | User review. Do not label v1.0 frozen without explicit approval. |
| 0.8 — Core syntax and scalar semantics | Add `func`/`var`/`const` syntax with compatibility policy; numeric primitives/literals/conversions; generic dot-member AST and semantic resolution scaffolding | Lexer/parser/AST and diagnostics first; test legacy syntax policy, inference, overflow and casts; no aggregate types required. |
| 0.9 — Aggregates and patterns | Structs, tuples including `.0`, arrays, enums, field/method distinction foundations, `match` statement and exhaustiveness | Target layouts and aggregate IR before backend lowering; static tuple bounds; enum exhaustiveness and guarded arms. |
| 0.10 — Ownership and unsafe | Move/copy analysis, references/lifetimes, borrow checker, partial moves, drop scopes/destructors, raw pointers/unsafe | Soundness gates in memory-model.md; cannot claim safety from syntax-only checks. |
| 0.11 — Generics and value errors | Generic functions/types, traits needed for constraints, monomorphization, function pointer types, Option/Result, exact `?` | Type/layout recursion guard and coherence first; exhaustive match already exists; same-type error propagation. |
| 0.12 — Methods and dynamic callables | `impl`, trait impl/default methods, function values; consider closure syntax after explicit capture and lifetime details | Generics/ownership required. Closures remain deferred until escape/capture tests and closure ABI are specified. `dyn Trait` remains deferred. |
| 0.13 — Modules and packages | Multi-file resolver, `import`, `pub import`, visibility, cycle diagnostics, `jahe.toml`, `jahe.lock`, separate interfaces | Complete symbol/export model first; deterministic version solver, hashes and locked builds; module cycles rejected. |
| 0.14+ — Standard library and platform expansion | Core/string/collections/I/O/filesystem/network/time, threads/sync, additional targets | Runtime/ABI contract per target; ownership wrappers; per-target test infrastructure. Async, atomics and FFI extensions need their design gates. |

## Dependency order

The key dependency graph is:

```text
primitive types + expression/name model
        ↓
aggregate layout + tuple projections + patterns
        ↓
ownership, lifetimes, borrow checking, destruction
        ↓
generics + trait constraints + Option/Result
        ↓
methods/function values + safe library APIs
        ↓
multi-module visibility/re-exports + locked package resolver
        ↓
standard library growth and additional platform ABIs
```

The order can change only when an implementation dependency is documented. `match`, block/if expressions, closures, async, unions, dynamic dispatch, static data, inline assembly, volatile/atomics, workspaces, registry trust lifecycle, and cross-platform ABI remain deferred; phase rows do not promise them by default. Compiler 0.7 native execution remains Windows x86_64/MSVC only until verified on another target.
