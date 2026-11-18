# JAHE compiler roadmap

This tracks compiler implementation history and near-term compiler work. The designed language architecture and its separate implementation proposal are in [language specification](language-specification.md) and [language roadmap](language-roadmap.md); proposed syntax is not implied to be implemented here.

## 0.1 — Bootstrap frontend

Rust Cargo workspace crate, source locations, lexer, parser, AST, diagnostics, and `check`, `tokens`, and `ast` CLI commands. No code generation.

## 0.2 — Semantic foundation (implemented)

Add name resolution, basic type checking, explicit semantic diagnostics, and tests for invalid types and unresolved names. Stabilize the AST/frontend boundary.

## 0.3 — Typed intermediate representation (implemented)

Define a backend-neutral typed IR and lower supported functions into it. Keep target-specific concerns behind a backend interface. Preserve semantic types and source spans through lowering, and validate IR invariants with tests.

## 0.4 — IR foundation release candidate (implemented)

Typed CFG/SSA foundation with `if`/`else`, `while`, phi nodes, expression spans, dominance and CFG validation, structured builders, return-path checks, and a minimal pass interface. Limitations are listed in [the IR design](ir-design.md).

## 0.5 — Backend foundation (implemented)

Backend trait, dedicated verifier API, captured-output IR interpreter, host-function registry, execution limits, and `run` command. See [backend design](backend-design.md). Execution is interpretation; no native output is generated.

## 0.5.1 — Backend contract hardening (implemented)

Backends consume verifier-produced `VerifiedModule` values. IR modules carry explicit, backend-neutral external signatures; the interpreter registry is signature-aware, rejects duplicate registrations, checks declarations before dispatch, and propagates structured host errors. The current JAHE call model is documented in [backend design](backend-design.md). No source-level external declaration syntax or native ABI is implemented.

## 0.6 — Cranelift native backend (implemented)

Add Cranelift 0.137.2 native code generation from `VerifiedModule`, emit Windows x86_64 COFF objects, and link executables with MSVC. The backend lowers arithmetic, comparisons, calls, block parameters/phi nodes, branches, and returns. The CLI exposes `build` while retaining interpreter execution through `run`. Native integration tests execute the generated executable and verify output and exit code. See [backend design](backend-design.md) for supported operations and limits.

## 0.7 — Runtime foundation (implemented)

Add a separate `jahe-runtime` crate with versioned C ABI v1, typed output, tracked allocation/reallocation/deallocation, and aborting panic support. Native builds statically link the runtime and `runtime-info` reports ABI and host capabilities. See [runtime ABI](runtime-abi.md) for contracts and limitations.

## Later

Grow the language deliberately: modules and imports, richer data types, ownership and borrowing rules, unsafe operations, standard-library components, and eventual self-hosting. Each feature requires specification, diagnostics, and tests before being treated as supported.
