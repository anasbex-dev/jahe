# JAHE compiler 0.7 accepted syntax snapshot

This page records what the current bootstrap frontend accepts. It is deliberately separate from the designed future language in [language-specification.md](language-specification.md). This is not the full design grammar.

## Current accepted forms

Each source file contains one module declaration and functions using `fn`:

```jahe
module hello;
fn add(a: i32, b: i32) -> i32 { return a + b; }
fn main() -> i32 { println(add(20, 22)); return 0; }
```

The supported source types are `i32`, `i64`, `f32`, `f64`, `bool`, `str`, and `void`. Function arguments and return types are explicit; omitted return type means `void`. Functions can call and recurse by declared name. Function overloading is rejected.

Statements include `let name: Type = expression;`, `let mut name: Type = expression;`, assignment to mutable locals, `return` with optional value, expression statements, nested blocks, `if`/`else`, and `while`. Declarations require initializers. Blocks create lexical scopes; nested scopes can shadow outer names, while same-scope duplicates are errors. There is no `var`, `const`, `import`, or source visibility syntax.

Expressions include identifiers, integer/float/string/bool literals, parentheses, calls, `+ - * /`, and comparisons `== != < <= > >=`. There are no unary operators or logical operators in the current parser. Numeric binary operands must have matching supported types; no implicit conversion occurs. Integer literals infer `i32` when possible, otherwise `i64`; floats infer `f64`. Strings support `\\n`, `\\t`, `\\r`, `\\"`, and `\\\\`. `//` comments are accepted. Identifiers use ASCII letters/underscore and digits after the first character.

`println` accepts one supported value and returns void. Semantic analysis checks declarations, names, argument counts/types, operators, assignments, conditions, and returns. Non-void functions require a return on all modeled paths; while loops are not assumed to execute.

## Backend behavior

The typed verified IR supports SSA values, functions/calls, constants, arithmetic/comparisons, phi nodes, branches and returns. The interpreter remains available. Native compilation uses Cranelift on Windows x86_64 and the runtime ABI described by [runtime-abi.md](runtime-abi.md). Native builds require `fn main() -> i32`; generated code supports only the native subset documented in [backend-design.md](backend-design.md).

## Not accepted today

The proposed canonical `func`, `var`, `const`, `import`, `pub`, struct/enum/trait/generic syntax, `for`/`loop`/`match`, references, pointers, ownership, unsafe blocks, source FFI, `Option`/`Result`, and standard-library modules are designed or deferred only. They must not be presented as implemented.
