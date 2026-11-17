use super::{BasicBlock, Type, Value};
use crate::lexer::token::Span;
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub value: Value,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Type,
    pub blocks: Vec<BasicBlock>,
    pub span: Span,
}
