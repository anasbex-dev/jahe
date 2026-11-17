use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Module,
    Fn,
    Let,
    Mut,
    Return,
    If,
    Else,
    While,
    True,
    False,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    Keyword(Keyword),
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(v) => write!(f, "identifier({v})"),
            Self::Integer(v) => write!(f, "integer({v})"),
            Self::Float(v) => write!(f, "float({v})"),
            Self::String(v) => write!(f, "string({v:?})"),
            Self::Keyword(k) => write!(f, "keyword({k:?})"),
            Self::Eof => write!(f, "EOF"),
            other => write!(
                f,
                "{}",
                match other {
                    Self::LeftParen => "(",
                    Self::RightParen => ")",
                    Self::LeftBrace => "{",
                    Self::RightBrace => "}",
                    Self::Colon => ":",
                    Self::Semicolon => ";",
                    Self::Comma => ",",
                    Self::Dot => ".",
                    Self::Arrow => "->",
                    Self::Plus => "+",
                    Self::Minus => "-",
                    Self::Star => "*",
                    Self::Slash => "/",
                    Self::Equal => "=",
                    Self::EqualEqual => "==",
                    Self::Bang => "!",
                    Self::BangEqual => "!=",
                    Self::Less => "<",
                    Self::LessEqual => "<=",
                    Self::Greater => ">",
                    Self::GreaterEqual => ">=",
                    _ => unreachable!(),
                }
            ),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.span.line, self.span.column, self.kind)
    }
}
