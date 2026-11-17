# JAHE Language and Compiler 0.4

This document describes syntax and semantic checks accepted by the bootstrap frontend. Semantic checking is not code generation.

## Modules and functions

Each file starts with one module declaration followed by zero or more functions:

```jahe
module hello;
fn main() -> i32 { return 0; }
```

Functions have named, typed parameters, an optional `-> type` return type (omitted means `void`), and a braced body. Supported types are `i32`, `i64`, `f32`, `f64`, `bool`, `str`, and `void`.

## Statements and control flow

The parser accepts typed variable declarations, returns, expression statements, nested blocks, `if`/`else`, and `while`. Statements end in semicolons except blocks/control-flow constructs. Each function and nested block creates a lexical scope. Nested blocks may shadow outer variables; duplicate names in one scope are errors. Declarations are immutable unless written `let mut`. Assignment is a statement and is permitted only for mutable locals.

```jahe
let count: i32 = 2;
return count;
println("hello");
let mut counter: i32 = 0;
while counter < 10 { counter = counter + 1; }
if counter == 10 { println(true); } else { println(false); }
```

Return values are optional. Variable initializers are required.

## Expressions and tokens

Expressions support identifiers, integer and decimal floating-point literals, strings, booleans, parenthesized expressions, calls, and binary `==`, `!=`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/` operators. Arithmetic requires matching numeric types. Comparisons require matching comparable types and produce `bool`. Integer literals infer `i32` when in range, otherwise `i64`; floating literals infer `f64`. Strings support `\\n`, `\\t`, `\\r`, `\\"`, and `\\\\`. `//` comments extend to the line end.

The semantic pass resolves variables, parameters, and declared functions, checks initializer, argument, operator, and return types, and reports duplicate declarations. The built-in `println` currently accepts one argument of any supported value type and returns `void`.

Identifiers use ASCII letters/underscore initially and ASCII letters/digits/underscore thereafter. All numeric literals are decimal. Integer literals must fit signed 64-bit range.

## Not implemented

There is no implicit numeric conversion, break/continue, short-circuit boolean syntax, user-defined types, imports, ownership checking, unsafe blocks, memory management implementation, LLVM integration, or native executable generation. Non-void functions need a return on all statically modeled paths: both arms of an `if/else` can establish this; a while loop is never assumed to execute. These unsupported features should not be assumed to work.

## Typed intermediate representation

The `ir` CLI command lowers semantically valid programs into a backend-neutral typed CFG/SSA IR. Functions contain uniquely numbered basic blocks, typed parameters, constants, binary and call instructions, phi nodes, branches, conditional branches, and return terminators. Mutable assignments are represented by new SSA values and phi nodes at control-flow merges and loop headers. Values carry explicit IR types and expression source spans. Calls use symbolic function names; `println` is a recognized built-in. The validator checks value definitions, use ordering/dominance, phi placement and inputs, signatures, references, instruction/result types, block structure, branch conditions/targets, reachability, and return types.

The current IR has no break/continue, short-circuit operators, global data, target layouts, machine types, ownership operations, or backend/code-generation rules. Its control-flow graph supports only the constructs listed above. It is an in-memory compiler representation and textual dump, not executable code. `void` return-path completeness is checked for reachable fallthrough; a `while` loop is conservatively allowed to exit.
