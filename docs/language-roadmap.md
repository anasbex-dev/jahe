# JAHE language implementation roadmap

This is a dependency-ordered proposal, not a schedule. Existing version numbers 0.1–0.7 are historical milestones; they do not imply the designed language features are implemented. This milestone adds design documentation only and does not change compiler behavior.

| Proposed phase | Scope | Dependencies / exit criteria |
|---|---|---|
| 0.7 (current baseline) | Runtime ABI v1, native Windows x64 backend | Implemented status is recorded in [runtime ABI](runtime-abi.md) and [backend design](backend-design.md). |
| 0.8 | `func`/`var` migration decision, primitive scalar expansion, literal/conversion/overflow specification and tests, UTF-8 string literal/slice semantics | Freeze primitive representation, errors and target-layout queries first. Keep legacy spellings until explicit migration/deprecation decision; no silent break. |
| 0.9 | Structs, tuples, arrays, enums, field access and layout; exhaustive `match`, patterns | Requires aggregate type checking, deterministic target layout, IR aggregate values and diagnostics. Recursive by-value layout errors included. |
| 0.10 | References, raw pointers, `unsafe` blocks, ownership/borrow checker, drops and safe allocation wrapper | Must complete soundness rules for control-flow joins, lifetimes, partial moves, destructors and FFI escape before claiming safe memory behavior. Raw memory APIs remain unsafe. |
| 0.11 | Generic functions/types, traits needed for bounds, function pointer types, `Option` and `Result` foundations | Requires monomorphization, recursive generic layout guards, coherence policy, generic enum representation, and tests. Postfix `?` only after conversion rules are stable. |
| 0.12 | Methods/`impl`, trait implementation/default methods, static dispatch, closures/function values if resolved | Requires generics/ownership; dynamic dispatch only after object-safety and vtable ABI decisions. Closures need capture/lifetime rules. |
| 0.13 | Multi-file module graph, `import`, `pub`, package manifest, interface caching, core library | Requires stable module identity/name visibility, dependency resolution, import cycle diagnostics, target-aware cache keys and initial `std.core/std.string`. |
| 0.14+ | `std.io`, filesystem, networking, collections, threading/synchronization; external package ecosystem; additional targets | Requires portable runtime/ABI strategy, error model, safe ownership wrappers and per-target CI. Specialized image/audio/protocol support belongs in packages. |

## Cross-cutting gates

- Keep parser/semantic/IR/backend/runtime boundaries separate. Add source syntax only after grammar and diagnostics are agreed.
- Preserve interpreter semantics and test parity where an operation is shared with native compilation.
- Treat public module interfaces and external ABI as versioned contracts; do not infer layout from the build host during cross compilation.
- Run tests for supported targets and report unsupported targets explicitly. No release milestone is complete from documentation alone.
- Review open decisions in [language-design-decisions.md](language-design-decisions.md) before each dependent phase. Dates are intentionally omitted.
