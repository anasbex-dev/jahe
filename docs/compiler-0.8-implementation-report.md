# JAHE Compiler 0.8 implementation report

## Executive summary

**Scalar status: PARTIAL. JAHE 0.8 status: PARTIAL.** Integer widths, f32/f64, bool logical operations, Unicode scalar char values, additive native char printing, prior-declaration constant references, and a limited `never`/divergence model are represented through applicable compiler layers and tested with Windows x64 native execution. The typed constant evaluator now preserves type at each expression node, uses checked wide integer operations, rounds f32 operations at each step, and reports categorized constant failures. The default Windows x64 ABI rejects i128/u128 function parameters and returns. Exhaustive scalar operator parity, broader boundary cases, and general never-flow analysis remain incomplete. Package version stays `0.7.0`.

The approved language design remains in [language-specification.md](language-specification.md); [language-spec.md](language-spec.md) records the actual compiler subset. Open design questions and recommended decisions are listed in [specification-decisions-0.8.md](specification-decisions-0.8.md).

## Implemented features

- Canonical `func`, immutable `let`, mutable `var`, typed top-level/local `const`, optional local annotations, local inference, calls, returns, assignments to local names, `if`, and `while`.
- `--legacy-syntax` opts into `fn` and `let mut`; canonical mode rejects those forms with guidance.
- Integer types `i8/i16/i32/i64/i128/isize` and `u8/u16/u32/u64/u128/usize` are represented distinctly in semantic analysis and typed IR. `isize/usize` use the explicit target pointer width; the only accepted target is Windows x86_64 MSVC (64-bit pointers).
- Integer digit separators are validated in the lexer. Integer tokens preserve the raw decimal spelling and radix metadata; magnitude parsing uses `u128` and retains larger spellings for range diagnostics. The approved grammar defines decimal literals only.
- Integer arithmetic is checked for overflow in the interpreter and native backend, including unsigned underflow, division/remainder by zero, signed minimum divided/remaindered by -1, and invalid shift counts. Signed comparisons/right shifts and unsigned comparisons/logical right shifts preserve their respective semantics. Native i128/u128 division and remainder use an unrolled restoring-division lowering because Cranelift 0.137.2 does not lower its i128 divide instruction on x64.
- Integer-to-integer casts are range checked in interpreter and native code. Integer/float casts cover the represented numeric types; exhaustive parity for all width pairs remains unfinished.
- Arithmetic `+ - * / %`, unary negation, comparisons, bitwise `& | ^`, shifts `<< >>`, and checked integer casts are represented for all twelve integer types. The interpreter supports all twelve; native Cranelift supports internal integer values and selects signed or unsigned operations from the JAHE type. Native i128/u128 division and remainder use restoring division. Typed wide constant operations are implemented with checked width-aware values; exhaustive cross-backend edge coverage remains incomplete.
- Explicit `as` conversions cover integer widths and the existing float types. Integer conversions are range checked; float-to-integer conversion checks finite/range bounds and truncates toward zero. Conversions are explicit in IR, verified, and executed in both backends. Statically known invalid casts diagnose during semantic analysis where the current constant evaluator can evaluate them.
- Character literals accept one Unicode scalar and the grammar's `n/r/t/0`, slash, quote, and `u{...}` escapes; invalid scalar values and malformed/multi-scalar literals are rejected. `char` is retained as a distinct type/IR/runtime value and represented as 32-bit in native code. Equality/inequality are supported; ordering is rejected because the approved specification does not define it. Native `println(char)` uses an additive ABI v1 `u32` export that rejects invalid scalar values.
- Boolean `!` and short-circuit `&&`/`||` lower to CFG blocks and phi values. Conditions remain strictly boolean; integer to boolean conversion is not implicit. Boolean equality/inequality remain supported.
- f32/f64 literals, inference/annotations, IEEE `+ - * /`, comparisons, negation, integer/float casts, interpreter execution, and native execution are supported. Remainder is currently integer-only; representative f32 stepwise rounding and f64 precision/special-value cases are tested, while broad rounding, underflow, overflow, and f32/native parity matrices remain untested.
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

All twelve integer types have checked runtime arithmetic, remainder, comparisons, bitwise operators, shifts, and integer cast lowering. Interpreter operations use checked `i128/u128` arithmetic plus explicit width bounds; native Cranelift selects signed or unsigned operations from the JAHE type. Native i128/u128 divide/remainder are compiler-lowered with restoring division; zero and signed min/-1 checks occur before the sequence. Integer-to-float conversion selects signedness; float-to-integer conversion checks finite/range bounds and truncates toward zero.

The compile-time evaluator uses `TypedConstant { ty, value }`: the type carries integer width/signedness and explicit target width for `isize/usize`; integer payloads use `IntegerBits(u128)` masked to that type; float payloads use f64 storage, with f32 values rounded and expanded back to f64 after each operation. Bool and char remain distinct variants. Constant environments preserve these typed values for prior-declaration references. Structured failures identify overflow, underflow, division by zero, signed minimum divided by -1, invalid shifts, out-of-range casts, invalid typed operations, unknown dependencies, cycles, and unsupported expressions. Diagnostics carry the operation expression span and type where available.

Tests cover i128/u128 boundaries and compound add/subtract/multiply/divide/remainder/shift/reference operations, all integer type literal endpoints, categorized failures, f32 intermediate rounding, f64 precision, signed zero, infinity, NaN comparisons, interpreter results, and a linked native executable using a wide constant and f32 result. Scalar conformance tests generate a typed constant/interpreter matrix for 14 numeric types: same-type arithmetic and comparisons, integer bitwise/shift operators, unary negation, bool NOT/equality, char equality, string equality, and all 196 numeric cast pairs. Semantic tests confirm bool/char numeric casts are rejected. The native matrix runs 414 numeric programs in isolated child processes: all supported integer/float operator cases and 188 supported numeric cast pairs. Five additional isolated programs cover bool NOT/equality and char equality. Each case checks a typed constant expectation through both the interpreter and linked native executable. These use representative in-range values; edge/error parity remains incomplete across every cell.

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

Top-level and local constants can refer to earlier constants in their declaration scope; dependency cycles receive diagnostics. Forward references remain rejected because the approved specification does not say whether they are allowed. The generated matrix validates constant evaluation and interpreter behavior with representative in-range operands; it does not provide boundary/error parity for every cell. Native execution likewise has representative operator/cast tests plus targeted i128/u128, IEEE float, and trap tests, rather than execution of all cast pairs and operators at every width.

### D — Member resolution: not implemented beyond AST scaffolding

The AST distinguishes generic member access, calls, and tuple-index syntax. There are no struct/tuple declarations or tuple value types, method/associated-item declarations, import AST, multi-module loader, module graph, or visibility symbol tables. Consequently field/method/tuple/module resolution cannot be made meaningful within the current architecture without adding those prerequisites. The design documents defer much of that functionality; this report does not claim it is implemented.

### E — Backend parity and stabilization: partial

The IR verifier checks currently modeled scalar types and operations; the interpreter and Cranelift backend execute the supported subset. Native executable tests cover 414 numeric operator/cast cases plus five bool/char cases, signed/unsigned integer families, i128/u128 internal operations including division/remainder/multiply, Unicode char equality/printing, f64 signed-zero/infinity/NaN behavior, boolean short-circuiting, never-control-flow paths, calls, casts, overflow traps, and invalid casts. Native string comparisons and i128/u128↔f32/f64 casts are rejected with NAT0002 before code generation. Exhaustive native parity at every scalar boundary remains uncovered.

## Architecture changes in this task

- Added `BinaryOp::Cast`, explicit conversion validation and lowering, checked interpreter conversions, and native conversion code including NaN/range guards for float-to-int.
- Added bitwise and shift tokens/precedence, signed integer semantic/verifier rules, checked interpreter operations, and Cranelift lowering with shift count/overflow traps.
- Preserved `%` and its tests; expanded interpreter/native coverage for casts, shifts, and the i32 minimum literal.
- Preserved the raw decimal integer architecture and expanded it from lexing into semantic typing, IR, interpreter, verifier, casts, and native operations for all twelve integer types.
- Added all twelve integer identities to semantic types and IR, a target specification with explicit pointer width and non-Windows-x64 rejection, width-aware literal validation and bit-pattern constants, checked interpreter operations/casts, and signed/unsigned Cranelift lowering. Added native and interpreter coverage through i128/u128 internal execution and precise i128 ABI rejection.
- Added Unicode scalar literal lexing/parsing and validation, `char` IR/interpreter/native values and equality, plus the additive ABI v1 native char-print service. Character ordering is now rejected until specified. Added boolean `!` and CFG-based short-circuit `&&`/`||`, explicit floating negation IR, and limited `never` CFG termination for exact `while true` and never-returning calls.
- Replaced the untyped compile-time evaluator path with a `TypedConstant` evaluator that propagates scalar types and target pointer width through nested expressions, performs checked width-aware i128/u128 arithmetic, rounds f32 per operation, and returns categorized diagnostics. Top-level and local constant lowering now consumes the same typed evaluator results. Added integer endpoint/error, f32/f64 IEEE, interpreter, and native executable regression coverage.
- Updated implementation snapshot, roadmap, README, and examples so documented/default CLI usage reflects canonical syntax and the actually implemented subset.

No runtime ABI, LLVM probe, or package dependency/version was changed.

## Verification run

All commands ran in the Windows x86_64/MSVC environment:

- `cargo check --offline --manifest-path compiler/Cargo.toml` — passed.
- `cargo check --offline --manifest-path runtime/Cargo.toml` — passed.
- `cargo test --offline --manifest-path compiler/Cargo.toml` — passed: 100 integration tests (24 backend, 12 frontend, 16 IR, 29 native, 19 semantic); no failures. The final run passed all 414 numeric and five bool/char isolated native conformance cases.
- `cargo test --offline --manifest-path runtime/Cargo.toml` — passed: 5 runtime integration tests, including invalid-scalar char-print rejection.
- `cargo fmt --check --manifest-path compiler/Cargo.toml` — passed.
- `cargo fmt --check --manifest-path runtime/Cargo.toml` — passed.
- CLI `check examples/hello.jahe` — passed.
- CLI `run examples/arithmetic.jahe` — printed `42`, exit code 0.
- CLI `build` typed-constant regression program — emitted a 796-byte COFF `.obj` and 155,648-byte `.exe`; MSVC linked it, and running it printed `0` (stepwise f32 rounding) with exit code 0. The native test also validates a u128 max-minus-one relation.
- Native integration tests generated COFF objects, linked Windows executables with MSVC, and executed them. The new native scalar smoke matrix checks arithmetic, remainder, shifts, comparisons, integer casts, and f32 operations/casts against typed constants. Other executables test Unicode char equality/printing, float signed-zero/infinity/NaN behavior, boolean short-circuit effects, and a never-returning CFG path. The legacy canonical program output is `42\n2\n3\n7\n12\n-2147483648\n`.
- Native stabilization isolated the prior crash to checked integer narrowing casts. In `native_integer_cast`, `bnot(valid)` treated Cranelift's bitwise complement as logical NOT, so even a valid `i16`→`i8` cast reached the emitted `ud2` trap. The child sometimes printed a stack-overflow message and sometimes exited with an illegal-instruction status; disassembly of the isolated COFF object showed the invalid predicate and `ud2`, with only a 16-byte stack frame and no recursive call. This was generated-code trap behavior, not harness recursion or stack exhaustion. Replacing the complement with `icmp NotEqual` fixed the narrowing casts. The i128 multiplication overflow predicate had the same boolean-normalization error; explicit comparisons and a nonzero divisor for the zero-product check now handle it. Isolated zero-product and signed/unsigned overflow regressions pass.
- The reusable harness gives each scalar case a distinct source, COFF object, executable, and child process; it captures exit code/stdout/stderr, verifies object/EXE creation, and continues after a child crash. All 414 numeric cases and five bool/char cases passed in the final full suite. Follow-up tests also pass for i128/u128 multiplication overflow, zero multiplication, runtime f32 rounding, and float-to-i32 limits, NaN, and infinities; valid min/max casts and invalid casts agree between interpreter and native execution.
- Native rejects string comparisons and the eight i128/u128↔f32/f64 casts with NAT0002; i128/u128 function parameters and returns remain rejected under the current Windows x64 ABI. The current coverage is full operator/cast cell coverage for ordinary values, not exhaustive boundary and error parity.

## Exact remaining work

1. Add exact edge/error conformance for each matrix cell, including broader pointer-sized and unsigned shift boundaries, float-to-int boundaries, and overflow/trap parity. The generated matrix currently uses ordinary in-range operands; forward constant references remain rejected until the approved spec defines their behavior.
2. Extend boundary/error parity to every operator and cast cell. The new native matrix establishes ordinary-value parity for all currently supported cells; it does not test each cell's min/max, overflow, underflow, invalid conversion, or exceptional float behavior.
3. Keep i128/u128 function ABI boundaries rejected unless a sound supported calling convention is established; the broad per-cell matrix exercises these types only as internal values.
4. Extend `never` beyond exact `while true` recognition to sound CFG reachability analysis and settle unreachable-code policy; broaden f32/f64 underflow/overflow and char boundary/diagnostic coverage.
5. Add tuple values/projections and the type/item/import/module/visibility foundations before claiming member, method, associated-item, or module resolution.

Until these are complete and tested, JAHE 0.8 remains **PARTIAL**, and the compiler package version remains `0.7.0`.

## Boundary and error parity follow-up

The boundary follow-up adds generated, per-case native executable comparisons for the 12 integer types (`i8/i16/i32/i64/i128/isize/u8/u16/u32/u64/u128/usize`; pointer width is 64 bits for the supported Windows x86_64 target). The operator suite checks signed/unsigned arithmetic at min/max, comparison and bitwise edges, shift edges, signed minimum remainder by `-1`, runtime add/subtract/multiply overflow, divide/remainder by zero, signed minimum divided by `-1`, negation, left-shift overflow, and invalid shift counts. Each case compares compile-time typed expectations where valid, interpreter behavior, and a separately generated COFF object and EXE; invalid runtime cases assert their interpreter error category and native trap status. Compile-time semantic tests check categorized failures for every integer width and out-of-range constants for all integer-to-integer type pairs.

The approved specification says signed minimum divided by `-1` panics but does not assign that behavior to remainder. Accordingly, `MIN % -1` is defined by the implementation as zero (the mathematical remainder) across constant evaluation, interpreter, and native code; `/ -1` remains an overflow error/trap. The previous host-language remainder panic and native IDIV fault were corrected and regression-tested for every signed integer width.

These additions do **not** complete the full requested matrix. Float cast boundary coverage is still only the existing focused `f64 -> i32` suite plus existing f32 rounding/IEEE cases, rather than every supported float-to-integer target at adjacent representable values. Exhaustive integer-to-float precision-loss boundaries, all f32/f64 overflow/underflow and subnormal edges, comprehensive signed-zero operation coverage, and every applicable “one beyond” integer boundary are not covered across all execution paths. Runtime error parity is validated for listed integer arithmetic cases, but no blanket proof is made for unsupported ABI/target cases. The compiler remains **PARTIAL** for boundary/error conformance.

Boundary follow-up verification before float coverage (Windows x86_64/MSVC): `cargo fmt --check`, `cargo check --offline`, and `cargo test --offline` passed for both `compiler` and `runtime`; compiler had 106 integration tests and runtime had 5. The integer operator/cast boundary tests used independently linked child EXEs. `jahe check examples/hello.jahe` passed, `jahe run examples/arithmetic.jahe` printed `42`, and `git diff --check` passed (Git emitted only line-ending normalization warnings).

## Floating-point conformance follow-up

The f32/f64 follow-up tests actual IEEE neighbors by constructing values from `next_up`/`next_down` bit patterns and serializing round-trip decimal inputs (the lexer has no exponent notation). For each f32 and f64 source, all ten native-supported integer cast targets (`i8/i16/i32/i64/isize/u8/u16/u32/u64/usize`) cover lower/upper range neighbors, signed zeros, ±0.5, smallest source subnormals, NaN, and both infinities. Each valid case compares a typed constant expectation, interpreter result, and its own COFF/EXE process; invalid runtime casts compare `InvalidCast`, native trap status, and compile-time `E0708` for a constant cast. `i128/u128` float cast boundaries are checked in constant evaluation and interpreter only; the native backend intentionally rejects those casts with its existing wide-float-cast diagnostic.

Integer-to-float tests cover all twelve integer source types to f32/f64, including endpoints and neighbors around 2^24 and 2^53. Constant and interpreter paths cover i128/u128; native isolated processes cover the other ten integer types. The typed expected float is compared after conversion (so IEEE rounding is accepted, rather than requiring integer preservation). f64-to-f32 widening/narrowing tests include max finite and adjacent values, overflow to infinity, min normal, largest/min subnormal, underflow, signed zero, NaN and infinities; f32-to-f64 tests include representative finite/subnormal and special values.

Arithmetic coverage includes all four supported operations for f32 and f64 around positive/negative zero, subnormal addition/cancellation, underflow, normal/subnormal transition, finite overflow, infinities, and NaN propagation. It prints the folded expected value and runtime result for comparison across constant evaluation, interpreter, and isolated native EXE; shortest round-trip formatting distinguishes finite bit patterns and signed zero. Native and interpreter NaN comparisons cover all six comparison operators in both operand orders. Existing f32 stepwise-rounding tests continue to check rounding after each operation.


The IR verifier previously rejected f32 infinity/NaN constants, causing valid IEEE conversions such as `u128::MAX as f32` to fail verification after constant folding. It now accepts non-finite f32 constants while retaining the existing finite magnitude range check. Verification after this fix passed: `cargo fmt --check`, `cargo check --offline`, and `cargo test --offline` for compiler and runtime. Compiler integration tests: 112 passed (24 backend, 12 frontend, 16 IR, 29 native, 3 integer-boundary native, 6 float-conformance, 22 semantic); runtime integration tests: 5 passed. The float suite itself passed 6/6; its native tests generated COFF objects, linked EXEs, and executed each case in a separate child process. Existing native scalar tests (29/29) and integer boundary tests (3/3) also passed in the complete compiler run. CLI check passed and interpreter output was `42`; `git diff --check` passed with line-ending warnings only. Raw NaN payload equality is not tested: JAHE currently exposes no float bitcast operation, so tests compare value behavior and round-trip display. The compiler remains **PARTIAL** for exhaustive NaN payloads and every arithmetic operand-pair combination; the tested coverage is not a proof over all floating-point bit patterns.
