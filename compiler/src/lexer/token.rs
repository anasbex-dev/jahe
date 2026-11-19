use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegerLiteral {
    /// Original decimal spelling, including source separators.
    pub raw: String,
    pub radix: u8,
}
impl IntegerLiteral {
    pub fn magnitude(&self) -> Option<u128> {
        u128::from_str_radix(&self.raw.replace('_', ""), u32::from(self.radix)).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Module,
    Func,
    Fn,
    Let,
    Var,
    Const,
    Mut,
    Return,
    As,
    If,
    Else,
    While,
    True,
    False,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(IntegerLiteral),
    Float(f64),
    Character(char),
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
    Percent,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    Ampersand,
    AmpAmp,
    Caret,
    Pipe,
    PipePipe,
    Tilde,
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
            Self::Integer(v) => write!(f, "integer({})", v.raw),
            Self::Float(v) => write!(f, "float({v})"),
            Self::Character(v) => write!(f, "character({v:?})"),
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
                    Self::Percent => "%",
                    Self::Equal => "=",
                    Self::EqualEqual => "==",
                    Self::Bang => "!",
                    Self::BangEqual => "!=",
                    Self::Less => "<",
                    Self::LessEqual => "<=",
                    Self::Greater => ">",
                    Self::GreaterEqual => ">=",
                    Self::ShiftLeft => "<<",
                    Self::ShiftRight => ">>",
                    Self::Ampersand => "&",
                    Self::AmpAmp => "&&",
                    Self::Caret => "^",
                    Self::Pipe => "|",
                    Self::PipePipe => "||",
                    Self::Tilde => "~",
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
