use crate::lexer::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub constants: Vec<Constant>,
    pub functions: Vec<Function>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
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
        ty: Option<Type>,
        value: Expr,
        mutable: bool,
        span: Span,
    },
    Const(Constant),
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
    Integer(crate::lexer::token::IntegerLiteral),
    Float(f64),
    Character(char),
    String(String),
    Bool(bool),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Member {
        base: Box<Expr>,
        member: String,
    },
    TupleIndex {
        base: Box<Expr>,
        index: u32,
    },
    Cast {
        value: Box<Expr>,
        target: Type,
    },
    Unary {
        op: String,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
}
