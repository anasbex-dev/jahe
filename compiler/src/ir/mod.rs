pub mod blocks;
pub mod builder;
pub mod control_flow;
pub mod functions;
pub mod instructions;
pub mod lower;
pub mod module;
pub mod passes;
pub mod printer;
pub mod ssa;
pub mod types;
pub mod validate;
pub mod values;
pub use blocks::BasicBlock;
pub use functions::{Function, Parameter};
pub use instructions::{BinaryOp, Instruction, InstructionKind, Terminator};
pub use module::{Diagnostic, Module};
pub use types::Type;
pub use values::{BlockId, Constant, Value, ValueId};
pub fn format(module: &Module) -> String {
    printer::format(module)
}
