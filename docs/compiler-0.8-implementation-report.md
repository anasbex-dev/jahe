# JAHE Compiler 0.8 implementation report

## Executive summary

**Scalar status: PARTIAL. JAHE 0.8 status: PARTIAL.** Integer widths, f32/f64, bool logical operations, Unicode scalar char values, additive native char printing, prior-declaration constant references, and a limited `never`/divergence model are represented through applicable compiler layers and tested with Windows x64 native execution. The default Windows x64 ABI rejects i128/u128 function parameters and returns. Typed wide compound evaluation, exhaustive scalar parity, and general never-flow analysis remain incomplete. Package version stays `0.7.0`.

The approved language design remains in [language-specification.md](language-specification.md); [language-spec.md](language-spec.md) records the actual compiler subset.

## Implemented features

- Canonical `func`, immutable `let`, mutable `var`, typed top-level/local `const`, optional local annotations, local inference, calls, returns, assignments to local names, `if`, and `while`.
- `--legacy-syntax` opts into `fn` and `let mut`; canonical mode rejects those forms with guidance.
- Integer types `i8/i16/i32/i64/i128/isize` and `u8/u16/u32/u64/u128/usize` are represented distinctly in semantic analysis and typed IR. `isize/usize` use the explicit target pointer width; the only accepted target is Windows x86_64 MSVC (64-bit pointers).
- Integer digit separators are validated in the lexer. Integer tokens preserve the raw decimal spelling and radix metadata; magnitude parsing uses `u128` and retains larger spellings for range diagnostics. The approved grammar defines decimal literals only.
- Integer arithmetic is checked for overflow in the interpreter and native backend, including unsigned underflow, division/remainder by zero, signed minimum divided/remaindered by -1, and invalid shift counts. Signed comparisons/right shifts and unsigned comparisons/logical right shifts preserve their respective semantics. Native i128/u128 division and remainder use an unrolled restoring-division lowering because Cranelift 0.137.2 does not lower its i128 divide instruction on x64.
- Integer-to-integer casts are range checked in interpreter and native code. Integer/float casts cover the represented numeric types; exhaustive parity for all width pairs remains unfinished.
- Arithmetic `+ - * / %`, unary negation, comparisons, bitwise `& | ^`, shifts `<< >>`, and checked integer casts are represented for all twelve integer types. The interpreter supports all twelve; native Cranelift supports internal integer values and selects signed or unsigned operations from the JAHE type. Native i128/u128 division and remainder use restoring division. Wide compound constant evaluation and exhaustive cross-backend edge coverage remain incomplete.
- Explicit `as` conversions cover integer widths and the existing float types. Integer conversions are range checked; float-to-integer conversion checks finite/range bounds and truncates toward zero. Conversions are explicit in IR, verified, and executed in both backends. Statically known invalid casts diagnose during semantic analysis where the current constant evaluator can evaluate them.
- Character literals accept one Unicode scalar and the grammar's `n/r/t/0`, slash, quote, and `u{...}` escapes; invalid scalar values and malformed/multi-scalar literals are rejected. `char` is retained as a distinct type/IR/runtime value and represented as 32-bit in native code. Equality/inequality are supported; ordering is rejected because the approved specification does not define it. Native `println(char)` uses an additive ABI v1 `u32` export that rejects invalid scalar values.
- Boolean `!` and short-circuit `&&`/`||` lower to CFG blocks and phi values. Conditions remain strictly boolean; integer to boolean conversion is not implicit. Boolean equality/inequality remain supported.
- f32/f64 literals, inference/annotations, IEEE `+ - * /`, comparisons, negation, integer/float casts, interpreter execution, and native execution are supported. Remainder is currently integer-only; extensive rounding, underflow, overflow, and f32/native parity cases remain untested.
- `void` is supported for absent function results and `return;`; void values cannot be stored. `never` is a no-value function result; exact `while true` loops and calls to never-returning functions terminate CFG paths with `Unreachable`. Parameters/stored values of type `never` are rejected. General proof of divergence is not implemented.
- Generic member-chain AST and distinct tuple-index AST syntax. Unresolved members receive diagnostics; this is scaffolding, not field/method/module resolution or tuple projection execution.
- The verified backend-neutral IR, interpreter, Cranelift 0.137.2 object backend, COFF/MSVC link workflow, and runtime ABI v1 remain in place.

## Scalar coverage

| Type | Status | Current support |
|---|---|---|
| `i8`, `i16` | Partial | Semantic ranges, typed IR, interpreter arithmetic/casts, and native arithmetic/casts. Native object execution is tested; boundary coverage is currently stronger in the interpreter. |
| `i32`, `i64` | Partial | Existing typed IR, checked arithmetic, comparisons, bitwise/shifts, casts, interpreter, and Cranelift. Legacy behavior/regression tests remain. |
| `i128` | Partial / ABI-limited | Internal constants, arithmetic, comparisons, overflow traps, interpreter, and native executable tests work. Function parameters/returns are rejected by the native backend because of the default Windows x64 ABI limitation. |
| `isize` | Partial | Width resolves from the explicit target model; Windows x86_64 MSVC uses 64 bits. Interpreter and native comparisons/casts are covered. Other targets are rejected. |
| `u8`, `u16`, `u32`, `u64` | Partial | Semantic ranges, typed IR, checked interpreter/native arithmetic, unsigned comparisons/shifts, and checked casts. Native end-to-end coverage exercises these widths. |
| `u128` | Partial / ABI-limited | Internal constants, arithmetic, comparisons, overflow traps, interpreter, and native executable tests work. Function parameters/returns are rejected with the same default Windows x64 ABI limitation. |
| `usize` | Partial | Width resolves from the explicit target model; Windows x86_64 MSVC uses 64 bits. Interpreter and native comparisons/casts are covered. Other targets are rejected. |
| `f32`, `f64` | Partial | Typed literals and values, IEEE arithmetic/negation/comparisons, explicit casts, interpreter and native support. f64 signed-zero/infinity/NaN and runtime negation are exercised across backends; f32 edge parity and comprehensive precision/rounding cases remain. `%` is rejected for floats. |
| `bool` | Partial | Literals, conditions, comparisons, `!`, and CFG-based short-circuit `&&`/`||` run in interpreter and native tests. Constant bool `!`/logical expressions and prior-declaration constant references work; broader logical edge coverage remains incomplete. |
| `char` | Partial | Unicode scalar literals/escapes, validation, constants, typed IR, interpreter, native 32-bit representation, equality/inequality, and native printing are tested. Ordering is rejected pending a language rule; casts are unsupported; exhaustive Unicode boundary coverage remains. |
| `void` | Partial | Supported as absence of a function result and for void calls/`return;`; rejected as a stored value. Return and expression-statement behavior are covered by existing tests. |
| `never` | Partial | No runtime value exists. Never-result functions, exact `while true` divergence, never-call CFG termination, and basic coercion into a value context are implemented/tested. General divergence analysis, unreachable-code policy, and broader branch/loop proof are incomplete. |

No row above qualifies as fully complete against the v1.0 scalar contract. The grammar defines decimal integer literals; radix prefixes are not specified. The raw token records radix `10` so the representation can preserve that fact without inventing syntax.

### Specification details to settle

The approved specification identifies `char` as a Unicode scalar but does not define relational ordering or char casts. Ordering is rejected, and casts remain unsupported. The additive `jahe_rt_v1_print_char(u32)` runtime symbol accepts exactly Unicode scalar values and preserves ABI v1 compatibility because it adds no layout or calling-convention changes. The specification does not define floating remainder, so `%` remains integer-only.

## Integer literal architecture

`IntegerLiteral` stores the original decimal spelling (including separators) and radix. Separator positions are checked by the lexer. `magnitude()` parses directly to `u128`; it never uses `f64` or host `i64`, and text outside `u128` remains intact for an explicit semantic range diagnostic. `Constant::IntegerBits(u128)` stores width-independent bit patterns; the instruction result type determines signedness and active width. Runtime values use that same bit-pattern invariant for newly added integer variants, while the legacy `I32`/`I64` runtime variants remain for API compatibility. Signed minima are represented as masked two's-complement bits without first converting the positive magnitude to a signed value.

## Numeric, casts, and constant evaluation

All twelve integer types have checked arithmetic, remainder, comparisons, bitwise operators, shifts, and integer cast lowering. Interpreter operations use checked `i128/u128` arithmetic plus explicit width bounds; native Cranelift selects signed or unsigned operations from the JAHE type. Native i128/u128 divide/remainder are compiler-lowered with restoring division; zero and signed min/-1 checks occur before the sequence. Integer-to-float conversion selects signedness; float-to-integer conversion checks finite/range bounds and truncates toward zero. The compile-time evaluator still has i64-centric paths for compound integer expressions, so constant/runtime parity for wide arithmetic is incomplete.

## Character, boolean, `never`, and `void`

Character literals produce exactly one Unicode scalar. The lexer accepts the specified escapes and rejects malformed or non-scalar Unicode escapes; IR/runtime store a Rust scalar `char`, and native code uses its Unicode code point in a 32-bit integer. Equality/inequality are supported; ordering and casts are rejected pending explicit rules. Native character printing uses an additive, ABI v1-compatible `u32` runtime export. Boolean negation and logical operators use typed IR and CFG short-circuit lowering. `void` is an absent result, never a value. `never` is represented as a no-value return type with limited exact-infinite-loop and call termination semantics; general divergence analysis is incomplete.

## IR, interpreter, and Cranelift

The verifier and typed IR retain integer width/signedness and char identity; `never` has no SSA value and is permitted only as a function result. The interpreter executes every integer type, char, floats, boolean logic, and never control flow within step limits. Cranelift generates and executes Windows x64 code for all integer widths as internal values, char equality/printing, IEEE float operations, boolean CFG logic, and a never-returning CFG path. No LLVM runtime dependency is used. Other target triples are rejected.

### I128 internal and ABI support

Cranelift 0.137.2 supports internal i128 constants, comparisons, shifts, and arithmetic used by the backend; its x64 `udiv.i128` lowering is unavailable, so JAHE emits a checked unrolled restoring-division sequence for signed/unsigned divide and remainder. Generated native executables cover i128/u128 literals, arithmetic, comparisons, shifts, division, remainder, zero-divisor traps, min/-1 traps, and overflow traps. The compiler explicitly rejects i128/u128 function parameters and returns with a diagnostic. It does not add a custom pair-of-u64 ABI. External ABI interoperability for i128/u128 is unsupported.

## Phase coverage

### B — Scalar type system: partial

The IR and interpreter represent all twelve integer types and preserve signedness/width. `isize/usize` use the `TargetSpec` pointer width; only the Windows x86_64 MSVC target (64-bit pointers) is accepted. Character values/printing and limited never control flow exist, with the limitations noted above. The approved grammar specifies decimal integer literals only; hexadecimal, binary, and octal prefixes are not part of the current contract.

I128/U128 typed values, internal operations, and native execution are implemented and tested; their native function parameter/return ABI is not. The native backend rejects those signatures under Cranelift's default Windows x64 ABI.

### C — Numeric semantics: partial

`%`, bitwise operators, shifts, comparisons, checked arithmetic, and checked integer casts operate on all twelve integer types in typed IR and interpreter; native Cranelift lowering selects signed or unsigned instructions and has native execution coverage for representative values in each width family. I128/U128 calls across function ABI boundaries are explicitly unsupported.

Top-level and local constants can refer to earlier constants in their declaration scope; dependency cycles receive diagnostics. Forward references remain rejected because the approved specification does not say whether they are allowed. Compile-time evaluation of compound integer expressions remains i64-centric and is not unified with runtime arithmetic, so wide/unsigned constant parity is incomplete. Exhaustive operator-by-operator parity tests across every width are still needed. Float, char, and boolean behavior has explicit coverage gaps.

### D — Member resolution: not implemented beyond AST scaffolding

The AST distinguishes generic member access, calls, and tuple-index syntax. There are no struct/tuple declarations or tuple value types, method/associated-item declarations, import AST, multi-module loader, module graph, or visibility symbol tables. Consequently field/method/tuple/module resolution cannot be made meaningful within the current architecture without adding those prerequisites. The design documents defer much of that functionality; this report does not claim it is implemented.

### E — Backend parity and stabilization: partial

The IR verifier checks currently modeled scalar types and operations; the interpreter and Cranelift backend execute the supported subset. Native executable tests cover signed/unsigned integer families, i128/u128 internal operations including division/remainder, Unicode char equality/printing, f64 signed-zero/infinity/NaN behavior, boolean short-circuiting, never control-flow paths, calls, casts, overflow traps, and invalid casts. Exhaustive scalar boundary parity and unimplemented member operations remain uncovered.

## Architecture changes in this task

- Added `BinaryOp::Cast`, explicit conversion validation and lowering, checked interpreter conversions, and native conversion code including NaN/range guards for float-to-int.
- Added bitwise and shift tokens/precedence, signed integer semantic/verifier rules, checked interpreter operations, and Cranelift lowering with shift count/overflow traps.
- Preserved `%` and its tests; expanded interpreter/native coverage for casts, shifts, and the i32 minimum literal.
- Preserved the raw decimal integer architecture and expanded it from lexing into semantic typing, IR, interpreter, verifier, casts, and native operations for all twelve integer types.
- Added all twelve integer identities to semantic types and IR, a target specification with explicit pointer width and non-Windows-x64 rejection, width-aware literal validation and bit-pattern constants, checked interpreter operations/casts, and signed/unsigned Cranelift lowering. Added native and interpreter coverage through i128/u128 internal execution and precise i128 ABI rejection.
- Added Unicode scalar literal lexing/parsing and validation, `char` IR/interpreter/native values and equality, plus the additive ABI v1 native char-print service. Character ordering is now rejected until specified. Added boolean `!` and CFG-based short-circuit `&&`/`||`, explicit floating negation IR, and limited `never` CFG termination for exact `while true` and never-returning calls.
- Updated implementation snapshot, roadmap, README, and examples so documented/default CLI usage reflects canonical syntax and the actually implemented subset.

No runtime ABI, LLVM probe, or package dependency/version was changed.

## Verification run

All commands ran in the Windows x86_64/MSVC environment:

- `cargo check --offline --manifest-path compiler/Cargo.toml` — passed.
- `cargo test --offline --manifest-path compiler/Cargo.toml` — passed: 81 integration tests (23 backend, 12 frontend, 16 IR, 18 native, 12 semantic); no failures.
- `cargo test --offline --manifest-path runtime/Cargo.toml` — passed: 5 runtime integration tests, including invalid-scalar char-print rejection.
- `cargo fmt --check --manifest-path compiler/Cargo.toml` — passed after formatting the added native constant-reference test.
- `cargo fmt --check --manifest-path runtime/Cargo.toml` — passed.
- CLI `check examples/hello.jahe` — passed.
- CLI `run examples/arithmetic.jahe` — printed `42`, exit code 0.
- CLI `build` on a temporary scalar program — emitted a 477-byte COFF `.obj` and 133,120-byte `.exe`; running it printed `42`, exit code 0.
- Native integration tests generated COFF objects, linked Windows executables with MSVC, and executed them. In addition to integer coverage, executables test Unicode char equality/printing, float signed-zero/infinity/NaN behavior, boolean short-circuit effects, and a never-returning CFG path. The legacy canonical program output is `42\n2\n3\n7\n12\n-2147483648\n`.

## Exact remaining work

1. Replace the remaining i64-centric constant evaluator with typed, width-aware evaluation for wide/unsigned compound expressions and nested scalar expressions. Earlier constant references and cycle diagnostics are implemented; forward references remain rejected until the approved spec defines their behavior.
2. Expand boundary and interpreter/native parity tests to every integer operator, cast pair, and overflow/error edge for all twelve types.
3. Add executable native tests for more i128/u128 operators; keep function ABI boundaries rejected unless a sound supported calling convention is established.
4. Extend `never` beyond exact `while true` recognition to sound general divergence analysis and settle unreachable-code policy; finish f32/f64 rounding/underflow/overflow and char boundary/diagnostic coverage.
5. Add tuple values/projections and the type/item/import/module/visibility foundations before claiming member, method, associated-item, or module resolution.

Until these are complete and tested, JAHE 0.8 remains **PARTIAL**, and the compiler package version remains `0.7.0`.
