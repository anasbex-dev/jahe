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

Bindings use `let` for immutable locals and `var` for mutable locals. Initializers are required. An annotation can be omitted when the initializer uniquely determines one of the currently supported types: `i32`, `i64`, `f32`, `f64`, `bool`, or `str`. `const` declarations require explicit types and supported compile-time scalar expressions. `void` cannot be stored in a local. Lexical scopes, shadowing, same-scope duplicate checks, and mutability checks are retained.

Expressions include names, integer/float/string/bool literals, calls, parentheses, unary integer/float negation, `+ - * / %`, integer `& | ^ << >>`, comparisons `== != < <= > >=`, and `as` casts among `i32`, `i64`, `f32`, and `f64`. Numeric literals support valid `_` digit separators. Integer tokens are currently limited to `i64`; floats default to `f64`. Signed `i32`/`i64` arithmetic, shifts, and represented casts are checked. Generic dot-member and tuple-projection syntax are represented in the AST, with semantic diagnostics marking them outside the current scalar subset. See the implementation report for precise limits.

Statements include local/constant declarations, assignment to local names, expression statements, nested blocks, `return`, `if`/`else`, and `while`. `println` accepts one supported scalar value and returns `void`. Checked arithmetic and native/interpreter parity cover only the signed integer IR types and operators currently represented, not the complete 0.8 scalar contract.

## Backends

The interpreter and typed verified IR remain available. Native compilation uses Cranelift on Windows x86_64 and the existing runtime ABI. Native builds require MSVC Build Tools (`cl.exe` and `link.exe`) and emit COFF objects and executables. Unsupported lowering is diagnosed. The full primitive set, conversions, checked arithmetic, and member resolution are not implemented.
