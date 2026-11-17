use super::{BlockId, Instruction, Terminator};
use crate::lexer::token::Span;
#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub name: String,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
    pub span: Span,
}
