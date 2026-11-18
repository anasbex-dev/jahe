# JAHE Compiler 0.8 implementation report

## Executive summary

**Status: PARTIAL — not 0.8 complete.** The compiler accepts canonical 0.8 declarations and carries the existing scalar subset through semantic analysis, verified IR, interpretation, and the Windows x64 Cranelift backend. This continuation added numeric casts for the existing four numeric types plus bitwise/shift operations. The required full primitive set and semantic member/module resolution remain incomplete, so the package version stays `0.7.0`.

The approved language design remains in [language-specification.md](language-specification.md); [language-spec.md](language-spec.md) records the actual compiler subset.

## Implemented features

- Canonical `func`, immutable `let`, mutable `var`, typed top-level/local `const`, optional local annotations, local inference, calls, returns, assignments to local names, `if`, and `while`.
- `--legacy-syntax` opts into `fn` and `let mut`; canonical mode rejects those forms with guidance.
- Existing primitive subset: `i32`, `i64`, `f32`, `f64`, `bool`, and `str`; `void` is supported as a function result, not a local.
- Integer digit separators are validated in the lexer. Integer literal representation is still `i64`, and therefore does not meet the full range requirement.
- Arithmetic `+ - * / %`, unary negation, comparisons, function calls, and scalar compile-time constant expressions within the existing type model. Signed i32/i64 add/subtract/multiply/divide/remainder/negation are checked in the existing interpreter/native pathways; division/remainder by zero and signed minimum divided/remaindered by -1 fail rather than wrap. Float arithmetic follows IEEE behavior.
- Integer bitwise `& | ^` and shifts `<< >>` for i32/i64. Shift counts are validated; signed right shifts sign-extend; left shifts trap on signed overflow. Interpreter/native tests cover valid operations, invalid counts, overflow, and negative right shift.
- Explicit `as` conversions among i32/i64/f32/f64. Narrowing and float-to-integer conversions are range checked; float-to-integer truncates toward zero. Conversions are explicit in IR, verified, and executed in both backends. Statically known invalid casts diagnose during semantic analysis.
- Semantic analysis and IR lowering use the same scalar compile-time evaluator, including casts. It remains limited to i64-backed integers; runtime evaluation is separate.
- Generic member-chain AST and distinct tuple-index AST syntax. Unresolved members receive diagnostics; this is scaffolding, not field/method/module resolution or tuple projection execution.
- The verified backend-neutral IR, interpreter, Cranelift 0.137.2 object backend, COFF/MSVC link workflow, and runtime ABI v1 remain in place.

## Phase coverage

### B — Scalar type system: partial

The IR and interpreter represent only i32/i64/f32/f64/bool/str/void. No unsigned, i8/i16, i128, isize/usize, char, or never source values exist. Integer tokens are parsed into i64 before type checking. The current type checker does not provide signedness-aware or arbitrary-width range rules.

Cranelift 0.137.2's codegen IR contains I128 support, but JAHE currently has no I128 internal type, value, arithmetic, comparison, or literal support. Separately, Cranelift's x64 ABI implementation explicitly rejects I128 function parameters/returns by default unless LLVM ABI extensions are enabled. JAHE therefore has neither I128 internal support nor ABI support. Adding I128 requires both a typed implementation and a documented JAHE ABI policy.

### C — Numeric semantics: partial

`%`, bitwise operators, and shifts parse, type-check for signed integer operands, lower to typed IR, verify, and execute in interpreter and Cranelift. Native lowering checks zero and signed min/-1 before division/remainder. Signed i32/i64 arithmetic, shifts, and casts have checked failure behavior.

Unsigned arithmetic and full-width semantics remain unsupported. Casts involving missing primitive types remain unsupported. Integer tokens remain i64; runtime and constant evaluation are separate implementations, with parity tested for the represented subset but not exhaustively.

### D — Member resolution: not implemented beyond AST scaffolding

The AST distinguishes generic member access, calls, and tuple-index syntax. There are no struct/tuple declarations or tuple value types, method/associated-item declarations, import AST, multi-module loader, module graph, or visibility symbol tables. Consequently field/method/tuple/module resolution cannot be made meaningful within the current architecture without adding those prerequisites. The design documents defer much of that functionality; this report does not claim it is implemented.

### E — Backend parity and stabilization: partial

The IR verifier checks currently modeled scalar types and operations; the interpreter and Cranelift backend execute the supported subset. Native parity tests cover a canonical program with calls, constants, output, casts, remainder, shifts, and the i32 minimum literal, plus trap cases for invalid casts and shifts. Unimplemented widths/member operations and exhaustive numeric boundaries still have no parity coverage.

## Architecture changes in this task

- Added `BinaryOp::Cast`, explicit conversion validation and lowering, checked interpreter conversions, and native conversion code including NaN/range guards for float-to-int.
- Added bitwise and shift tokens/precedence, signed integer semantic/verifier rules, checked interpreter operations, and Cranelift lowering with shift count/overflow traps.
- Preserved `%` and its tests; expanded interpreter/native coverage for casts, shifts, and the i32 minimum literal.
- Updated implementation snapshot, roadmap, README, and examples so documented/default CLI usage reflects canonical syntax and the actually implemented subset.

No runtime ABI, LLVM probe, or package dependency/version was changed.

## Verification run

All commands ran in the Windows x86_64/MSVC environment:

- `cargo check --offline --manifest-path compiler/Cargo.toml` — passed.
- `cargo test --offline --manifest-path compiler/Cargo.toml` — passed: 51 integration tests (13 backend, 9 frontend, 15 IR, 6 native, 8 semantic); no failures.
- `cargo test --offline --manifest-path runtime/Cargo.toml` — passed: 4 runtime tests; no failures.
- `cargo fmt --check --manifest-path compiler/Cargo.toml` — passed.
- `cargo fmt --check --manifest-path runtime/Cargo.toml` — passed.
- CLI `check examples/hello.jahe` — passed.
- CLI `run examples/arithmetic.jahe` — printed `42`, exit code 0.
- CLI `build examples/functions.jahe` — emitted COFF object and executable; running the executable printed `7`, exit code 0.
- Native integration test — emitted `.obj`, linked `.exe` through MSVC, executed it, and checked stdout `42\n2\n3\n7\n12\n-2147483648\n` and exit code 0. Separate native executables trapped on an invalid narrowing cast and invalid/overflowing shifts.

## Exact remaining work

1. Preserve raw integer lexemes and implement contextual i8/i16/i32/i64/i128/u8/u16/u32/u64/u128/isize/usize literal range checks, including signed minima beyond the current i32 special case.
2. Add signedness/width-aware IR types and constants; implement `char` validation and `never` control-flow typing; define target pointer-width behavior.
3. Extend casts to all specified signed/unsigned widths and target ABI rules.
4. Implement unsigned arithmetic and expand bitwise/shift behavior to all integer widths and signedness.
5. Unify constant/runtime numeric semantics or establish exhaustive parity for each supported type/operator pair.
6. Add tuple values/projections and the type/item/import/module/visibility foundations before claiming member, method, associated-item, or module resolution.
7. Add exhaustive boundary, invalid-operation, verifier, interpreter/native parity, and diagnostic tests for every newly supported operation and type.

Until these are complete and tested, JAHE 0.8 remains **PARTIAL**, and the compiler package version remains `0.7.0`.
