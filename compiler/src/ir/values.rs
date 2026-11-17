use crate::lexer::token::Span;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub id: ValueId,
    pub ty: super::Type,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
}
