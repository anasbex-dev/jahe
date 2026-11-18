# JAHE IR design (0.4 foundation)

The IR belongs to JAHE and is independent of Cranelift and target architecture. Frontend lowering consumes the typed AST only after semantic analysis succeeds. The interpreter and Cranelift native backend consume this same IR through backend-specific adapters; neither backend types nor target layout rules belong in the IR model.

## Architecture

`compiler/src/ir/` separates types, values, instructions, blocks, functions, modules, builders, lowering, control-flow helpers, SSA helpers, validation, passes, and printing. `ir::lower::lower` produces a module or diagnostics and never returns a partially lowered module. `ir::validate::validate` is also available to builders and future passes. A minimal `Pass` trait and validation pass establish the extension point without speculative optimizations.

## Values and instructions

IR values have a function-unique `ValueId`, one explicit non-void value type (`i32`, `i64`, `f32`, `f64`, `bool`, or `str`), and source span. `void` is valid as a function result type and never as an SSA value type. Parameters define values at function entry. Constants, arithmetic/comparison operations, calls, and phis define instruction results. Void calls have no result. The current IR expresses local variables through SSA values; no memory slots or target representation are implied.

Arithmetic requires equal numeric operand types and returns that type. Comparisons require equal supported comparable types and return `bool`. Calls must match declared signatures; `println` is the one built-in and accepts one value. There are no implicit conversions.

## Blocks and control flow

Each function owns its own numbered blocks and entry block 0. A block contains instructions followed by one terminator: return, unconditional branch, or conditional branch. Conditional conditions are `bool`; targets must belong to the function. The initial frontend lowers `if`/`else` and `while`. Loops conservatively retain an exit edge because the compiler does not prove a condition always true.

## SSA and phi nodes

Each value ID has one definition. Ordinary uses must be defined earlier in the same block or by a definition that dominates the use block. Phi nodes must appear before ordinary instructions. A phi has exactly one typed incoming value for every CFG predecessor; incoming definitions must dominate their predecessor edge. Branch merges use phis for mutable locals assigned different values on open paths. Loop headers use phis for mutable values carried around the backedge. Immutable locals and parameters cannot be assigned.

## Source locations and diagnostics

Every expression now stores the source position of its first token. Lowered instructions and generated blocks retain relevant expression, statement, or declaration positions. Validation diagnostics report the stored span with an IR-specific code. The lexer currently records line and column (not byte offsets or end ranges).

## Validation

Validation checks unique function/value/block IDs, entry existence, parameter signatures, one terminator per block, known branch targets, reachable blocks, conditional types, instruction types, call signatures, returns, SSA definitions/use ordering/dominance, phi placement, phi types, and predecessor correspondence. Function-local block IDs make cross-function targets unrepresentable through the normal IR data model; validators resolve every target only within the containing function. Malformed structures produce diagnostics rather than panics.

## Current limits

There are no `break`, `continue`, short-circuit boolean operators, switch/match, global data, aggregate types, memory operations, ownership operations, or code generation. The CFG currently has no exception/unwind edges. Return completeness is enforced by rejecting reachable fallthrough from non-void functions; no general path proof for infinite loops is attempted. The textual printer is deterministic, but it is a debug/compiler format rather than a stable external interchange specification.
