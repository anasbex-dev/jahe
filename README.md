# JAHE

JAHE is an experimental compiled systems programming language inspired by C and Rust. Its long-term design aims include native compilation, ownership-based memory safety, hybrid memory management, explicit low-level control through unsafe operations, and an optional runtime. JAHE is not a scripting language or a wrapper around Rust.

## Status

Compiler 0.4.0 is the IR foundation release candidate. The Rust bootstrap compiler includes the JAHE 0.1 frontend, JAHE 0.2 semantic checks, and a backend-neutral typed CFG/SSA IR. `check` runs syntax and semantic checks; `ir` lowers supported source to readable IR. No command generates native code. The project is experimental and not production-ready. Unsupported syntax is documented in [the language specification](docs/language-spec.md).

## Build

Install a current stable Rust toolchain, then build from the repository root:

```sh
cargo build --manifest-path compiler/Cargo.toml
```

## CLI

```sh
cargo run --manifest-path compiler/Cargo.toml -- --version
cargo run --manifest-path compiler/Cargo.toml -- check examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- tokens examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ast examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ir examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ir examples/control_flow.jahe
```

`tokens` prints the lexical stream, `ast` prints the parsed syntax tree, `check` performs parsing and semantic validation, and `ir` prints validated typed IR. There is no native compilation command yet.

## Design philosophy

The bootstrap compiler keeps lexing, parsing, AST definitions, diagnostics, semantic analysis, typed IR, lowering, and validation in separate modules. The IR models control flow and SSA values without LLVM or target-machine dependencies and can serve as a boundary for multiple future backends. See [the IR design](docs/ir-design.md). Small, explicit milestones are preferred over claiming unfinished language features.

## Project map

- `compiler/`: Rust bootstrap compiler.
- `runtime/`: reserved for future runtime components.
- `std/`: reserved for a future standard library.
- `examples/`: sample JAHE source.
- `docs/`: syntax and milestone roadmap.

See [docs/roadmap.md](docs/roadmap.md) for planned next steps.
