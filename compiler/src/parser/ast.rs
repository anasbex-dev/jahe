use crate::lexer::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub span: Span,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Statement>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub span: Span,
    pub ty: Type,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type(pub String);
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Let {
        name: String,
        ty: Type,
        value: Expr,
        mutable: bool,
        span: Span,
    },
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    Return(Option<Expr>, Span),
    Expression(Expr, Span),
    Block(Vec<Statement>, Span),
    If {
        condition: Expr,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<Statement>,
        span: Span,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Name(String),
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}
