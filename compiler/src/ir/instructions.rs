use super::{BlockId, Constant, Type, Value};
use crate::lexer::token::Span;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum InstructionKind {
    Constant(Constant),
    Binary {
        op: BinaryOp,
        left: Value,
        right: Value,
    },
    Call {
        function: String,
        args: Vec<Value>,
    },
    Phi {
        incoming: Vec<(BlockId, Value)>,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub result: Option<Value>,
    pub kind: InstructionKind,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    Return(Option<Value>),
    Branch(BlockId),
    ConditionalBranch {
        condition: Value,
        then_block: BlockId,
        else_block: BlockId,
    },
    Unreachable,
}
#[allow(dead_code)]
fn _signature(_: Type) {}
