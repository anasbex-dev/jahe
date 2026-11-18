# JAHE

JAHE is an experimental compiled systems programming language inspired by C and Rust. Its long-term design aims include native compilation, ownership-based memory safety, hybrid memory management, explicit low-level control through unsafe operations, and an optional runtime. JAHE is not a scripting language or a wrapper around Rust.

## Status

Compiler 0.8 development adds canonical `func`/`let`/`var` syntax, inferred local bindings, scalar checks, numeric casts, remainder, bitwise/shift operations, and compile-time scalar constants to the existing verified IR, interpreter, and Cranelift native backend. The 0.8 milestone is still partial: see the [implementation report](docs/compiler-0.8-implementation-report.md) for the supported subset and open gaps. Native builds emit COFF objects and link the static `jahe-runtime` library through MSVC on Windows x86_64. The project is experimental and not production-ready.

## Build

Install a current stable Rust toolchain, then build from the repository root:

```sh
cargo build --manifest-path compiler/Cargo.toml
```

## CLI

```sh
cargo run --manifest-path compiler/Cargo.toml -- --version
cargo run --manifest-path compiler/Cargo.toml -- runtime-info
cargo run --manifest-path compiler/Cargo.toml -- check examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- tokens examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ast examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ir examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- ir examples/control_flow.jahe
cargo run --manifest-path compiler/Cargo.toml -- run examples/hello.jahe
cargo run --manifest-path compiler/Cargo.toml -- run examples/arithmetic.jahe
cargo run --manifest-path compiler/Cargo.toml -- run examples/functions.jahe
cargo run --manifest-path compiler/Cargo.toml -- run examples/control_flow.jahe
cargo run --manifest-path compiler/Cargo.toml -- run examples/loops.jahe
cargo run --manifest-path compiler/Cargo.toml -- build examples/arithmetic.jahe
```

`tokens` prints the lexical stream, `ast` prints the parsed syntax tree, `check` performs parsing and semantic validation, `ir` prints validated typed IR, `run` interprets a module's `main` function, and `build` emits `examples/arithmetic.obj` and `examples/arithmetic.exe`. Native builds currently require a Windows x86_64 host and Visual Studio C++ Build Tools (including `cl.exe` and `link.exe`). Use `--legacy-syntax` before the command to accept the old `fn` and `let mut` forms. An `i32 main` return in the range 0–255 becomes the process exit status when interpreted; a `void main` exits with 0. Native executable entry points must be `func main() -> i32`. Unsupported operations and external host functions are diagnosed instead of generating guessed code.

## Design philosophy

The bootstrap compiler keeps lexing, parsing, AST definitions, diagnostics, semantic analysis, typed IR, verification, backends, and interpretation in separate modules. Backends consume a `VerifiedModule` created by the independent IR verifier, so normal backend calls cannot receive unverified IR. Modules declare external functions with backend-neutral signatures; host implementations are registered separately by the interpreter. Host functions are not automatically available to JAHE source programs, and source-level external declarations are not yet supported. See [the IR design](docs/ir-design.md) and [backend design](docs/backend-design.md). Small, explicit milestones are preferred over claiming unfinished language features.

## Project map

- `compiler/`: Rust bootstrap compiler.
- `runtime/`: versioned `jahe-runtime` ABI crate with static library, I/O, memory, and panic services.
- `std/`: reserved for a future standard library.
- `examples/`: sample JAHE source.
- `docs/`: syntax and milestone roadmap.

See [the implementation roadmap](docs/roadmap.md) for compiler history and [the language roadmap](docs/language-roadmap.md) for proposed language phases. The [specification finalization report](docs/specification-finalization-report.md) records the initial design closure; the [final technical review](docs/specification-v1-final-review.md) includes the soundness corrections and review recommendation. The [syntax reference](docs/syntax-reference.md), [grammar](docs/grammar.ebnf), [design decisions](docs/language-design-decisions.md), [type system](docs/type-system.md), [memory model](docs/memory-model.md), [module system](docs/module-system.md), [error handling](docs/error-handling.md), and [standard library design](docs/standard-library-design.md) describe proposed features, not current compiler support.
