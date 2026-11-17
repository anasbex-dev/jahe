# JAHE roadmap

## 0.1 — Bootstrap frontend

Rust Cargo workspace crate, source locations, lexer, parser, AST, diagnostics, and `check`, `tokens`, and `ast` CLI commands. No code generation.

## 0.2 — Semantic foundation (implemented)

Add name resolution, basic type checking, explicit semantic diagnostics, and tests for invalid types and unresolved names. Stabilize the AST/frontend boundary.

## 0.3 — Typed intermediate representation (implemented)

Define a backend-neutral typed IR and lower supported functions into it. Keep target-specific concerns behind a backend interface. Preserve semantic types and source spans through lowering, and validate IR invariants with tests.

## 0.4 — IR foundation release candidate (implemented)

Typed CFG/SSA foundation with `if`/`else`, `while`, phi nodes, expression spans, dominance and CFG validation, structured builders, return-path checks, and a minimal pass interface. Limitations are listed in [the IR design](ir-design.md).

## 0.5 — Backend interface and IR consumer prototype

Define a stable backend consumer interface and prototype a non-generating verifier or interpreter for the supported IR. LLVM can later be one consumer. Do not advertise native output until machine-code generation and execution tests pass.

## Later

Grow the language deliberately: modules and imports, control flow, data types, ownership and borrowing rules, unsafe operations, runtime/memory components, and eventual self-hosting. Each feature requires specification, diagnostics, and tests before being treated as supported.
