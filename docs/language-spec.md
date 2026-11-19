# JAHE compiler implementation snapshot

This page describes the source subset currently accepted by the bootstrap compiler. It is intentionally distinct from the complete design contract in [language-specification.md](language-specification.md). JAHE 0.8 remains partial; see the [0.8 implementation report](compiler-0.8-implementation-report.md).

## Accepted syntax

Each file has one `module` declaration and functions use canonical `func` syntax:

```jahe
module hello;
const BASE: i32 = 40;
func add(a: i32, b: i32) -> i32 { return a + b; }
func main() -> i32 {
    let answer = add(BASE, 2);
    println(answer);
    return 0;
}
```

The frontend also has an explicit `--legacy-syntax` mode for the 0.7 `fn` and `let mut` forms. Canonical mode rejects those forms. Function parameters require types; omitted return type means `void`. Function signatures are collected before body analysis, so forward calls work. Overloading is not supported.

Bindings use `let` for immutable locals and `var` for mutable locals. Initializers are required. An annotation can be omitted when the initializer uniquely determines a supported type. Integer annotations include signed `i8/i16/i32/i64/i128/isize` and unsigned `u8/u16/u32/u64/u128/usize`; inference defaults to the smallest signed `i32`, `i64`, or `i128`, and larger positive magnitudes require an explicit unsigned context. `isize/usize` are 64-bit for the only accepted target, Windows x86_64 MSVC. `f32`, `f64`, `bool`, `char`, and `str` have parser, typed IR, interpreter, and native support for the subset in the implementation report. Character values are Unicode scalars represented natively as 32-bit values; equality and native printing are supported, while ordering and casts are rejected pending explicit language rules. `never` is supported for never-returning functions and exact `while true` control flow only. `void` cannot be stored in a local. Lexical scopes, shadowing, same-scope duplicate checks, and mutability checks are retained.

Expressions include names, integer/float/string/bool/character literals, calls, parentheses, unary integer/float negation and boolean `!`, `&&`/`||` with short-circuit control flow, `+ - * / %`, integer `& | ^ << >>`, comparisons `== != < <= > >=` (char supports equality only), and explicit casts among integer widths and float types. Numeric literals support valid `_` separators and preserve raw decimal text through `u128::MAX`; values beyond it produce range diagnostics. Integer arithmetic and casts are checked in the interpreter and Cranelift backend for supported native internal values. Native i128/u128 division uses compiler-generated restoring division; i128/u128 function parameters and returns are rejected under the default Windows x64 ABI. Float arithmetic follows IEEE operations; special-value behavior is exercised on both backends, while exhaustive precision/rounding parity remains incomplete. The approved grammar specifies decimal integer literals only. Floats default to `f64`. Generic dot-member and tuple-projection syntax are represented in the AST, with semantic diagnostics marking them outside the current scalar subset. See the implementation report for precise limits.

Statements include local/constant declarations, assignment to local names, expression statements, nested blocks, `return`, `if`/`else`, and `while`. Prior top-level and local constants can be referenced; cycles are diagnosed, and unresolved forward references are currently rejected because the specification does not state whether they are allowed. `println` returns `void`; the runtime ABI v1 has an additive `u32` char-print export, while unsupported integer widths still lack print helpers. Typed wide constant evaluation and exhaustive native/interpreter parity remain incomplete. `never` flow analysis is limited; see the implementation report.

## Backends

The interpreter and typed verified IR remain available. Native compilation uses Cranelift on Windows x86_64 and the existing runtime ABI. Native builds require MSVC Build Tools (`cl.exe` and `link.exe`) and emit COFF objects and executables. Unsupported lowering is diagnosed. The full primitive set, conversions, checked arithmetic, and member resolution are not implemented.
