pub mod token;

use crate::diagnostics::Diagnostic;
use token::{Keyword, Span, Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    chars: Vec<char>,
    offset: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.chars().collect(),
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.offset).copied()
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.offset + 1).copied()
    }
    fn advance(&mut self) -> Option<char> {
        let c = self.current()?;
        self.offset += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }
    fn span(&self, line: usize, column: usize) -> Span {
        Span { line, column }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
                continue;
            }
            let (line, column) = (self.line, self.column);
            if c == '/' && self.peek() == Some('/') {
                while self.current().is_some_and(|ch| ch != '\n') {
                    self.advance();
                }
                continue;
            }
            let kind = if c.is_ascii_alphabetic() || c == '_' {
                self.identifier()
            } else if c.is_ascii_digit() {
                self.number(line, column)?
            } else if c == '"' {
                self.string(line, column)?
            } else if c == '\'' {
                self.character(line, column)?
            } else {
                self.punctuation(line, column)?
            };
            tokens.push(Token {
                kind,
                span: self.span(line, column),
            });
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.span(self.line, self.column),
        });
        let _ = self.source;
        Ok(tokens)
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.offset;
        while self
            .current()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.advance();
        }
        let text: String = self.chars[start..self.offset].iter().collect();
        match text.as_str() {
            "module" => TokenKind::Keyword(Keyword::Module),
            "func" => TokenKind::Keyword(Keyword::Func),
            "fn" => TokenKind::Keyword(Keyword::Fn),
            "let" => TokenKind::Keyword(Keyword::Let),
            "var" => TokenKind::Keyword(Keyword::Var),
            "const" => TokenKind::Keyword(Keyword::Const),
            "mut" => TokenKind::Keyword(Keyword::Mut),
            "as" => TokenKind::Keyword(Keyword::As),
            "return" => TokenKind::Keyword(Keyword::Return),
            "if" => TokenKind::Keyword(Keyword::If),
            "else" => TokenKind::Keyword(Keyword::Else),
            "while" => TokenKind::Keyword(Keyword::While),
            "true" => TokenKind::Keyword(Keyword::True),
            "false" => TokenKind::Keyword(Keyword::False),
            _ => TokenKind::Identifier(text),
        }
    }

    fn number(&mut self, line: usize, column: usize) -> Result<TokenKind, Diagnostic> {
        let start = self.offset;
        while self
            .current()
            .is_some_and(|c| c.is_ascii_digit() || c == '_')
        {
            if self.current() == Some('_')
                && (!self.peek().is_some_and(|c| c.is_ascii_digit())
                    || self.offset == start
                    || !self.chars[self.offset - 1].is_ascii_digit())
            {
                return Err(Diagnostic::new(
                    "numeric separators must appear between digits",
                    self.span(self.line, self.column),
                ));
            }
            self.advance();
        }
        let float = self.current() == Some('.') && self.peek().is_some_and(|c| c.is_ascii_digit());
        if float {
            self.advance();
            while self
                .current()
                .is_some_and(|c| c.is_ascii_digit() || c == '_')
            {
                if self.current() == Some('_')
                    && (!self.peek().is_some_and(|c| c.is_ascii_digit())
                        || !self.chars[self.offset - 1].is_ascii_digit())
                {
                    return Err(Diagnostic::new(
                        "numeric separators must appear between digits",
                        self.span(self.line, self.column),
                    ));
                }
                self.advance();
            }
        }
        let raw: String = self.chars[start..self.offset].iter().collect();
        let text = raw.replace('_', "");
        if float {
            text.parse().map(TokenKind::Float).map_err(|_| {
                Diagnostic::new("invalid floating-point literal", self.span(line, column))
            })
        } else {
            Ok(TokenKind::Integer(token::IntegerLiteral { raw, radix: 10 }))
        }
    }

    fn string(&mut self, line: usize, column: usize) -> Result<TokenKind, Diagnostic> {
        self.advance();
        let mut value = String::new();
        loop {
            match self.advance() {
                Some('"') => return Ok(TokenKind::String(value)),
                Some('\\') => match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some(other) => {
                        return Err(Diagnostic::new(
                            format!("unsupported escape sequence \\{other}"),
                            self.span(self.line, self.column.saturating_sub(1)),
                        ))
                    }
                    None => break,
                },
                Some('\n') | None => break,
                Some(c) => value.push(c),
            }
        }
        Err(Diagnostic::new(
            "unterminated string literal",
            self.span(line, column),
        ))
    }

    fn character(&mut self, line: usize, column: usize) -> Result<TokenKind, Diagnostic> {
        self.advance();
        let value = match self.advance() {
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('0') => '\0',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('"') => '"',
                Some('u') if self.current() == Some('{') => {
                    self.advance();
                    let start = self.offset;
                    while self.current().is_some_and(|c| c.is_ascii_hexdigit()) {
                        self.advance();
                    }
                    let digits: String = self.chars[start..self.offset].iter().collect();
                    if digits.is_empty() || self.current() != Some('}') {
                        return Err(Diagnostic::new(
                            "invalid Unicode escape in character literal",
                            self.span(line, column),
                        ));
                    }
                    self.advance();
                    let scalar = u32::from_str_radix(&digits, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| {
                            Diagnostic::new(
                                "Unicode escape is not a valid scalar value",
                                self.span(line, column),
                            )
                        })?;
                    scalar
                }
                Some(other) => {
                    return Err(Diagnostic::new(
                        format!("unsupported character escape \\{other}"),
                        self.span(line, column),
                    ))
                }
                None => {
                    return Err(Diagnostic::new(
                        "unterminated character literal",
                        self.span(line, column),
                    ))
                }
            },
            Some(c) if c != '\n' && c != '\r' && c != '\'' => c,
            _ => {
                return Err(Diagnostic::new(
                    "character literal must contain exactly one Unicode scalar",
                    self.span(line, column),
                ))
            }
        };
        if self.current() != Some('\'') {
            return Err(Diagnostic::new(
                "character literal must contain exactly one Unicode scalar",
                self.span(line, column),
            ));
        }
        self.advance();
        Ok(TokenKind::Character(value))
    }

    fn punctuation(&mut self, line: usize, column: usize) -> Result<TokenKind, Diagnostic> {
        let c = self.advance().unwrap();
        let kind = match c {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '-' if self.current() == Some('>') => {
                self.advance();
                TokenKind::Arrow
            }
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '=' if self.current() == Some('=') => {
                self.advance();
                TokenKind::EqualEqual
            }
            '=' => TokenKind::Equal,
            '!' if self.current() == Some('=') => {
                self.advance();
                TokenKind::BangEqual
            }
            '!' => TokenKind::Bang,
            '<' if self.current() == Some('=') => {
                self.advance();
                TokenKind::LessEqual
            }
            '<' if self.current() == Some('<') => {
                self.advance();
                TokenKind::ShiftLeft
            }
            '<' => TokenKind::Less,
            '>' if self.current() == Some('=') => {
                self.advance();
                TokenKind::GreaterEqual
            }
            '>' if self.current() == Some('>') => {
                self.advance();
                TokenKind::ShiftRight
            }
            '>' => TokenKind::Greater,
            '&' if self.current() == Some('&') => {
                self.advance();
                TokenKind::AmpAmp
            }
            '&' => TokenKind::Ampersand,
            '^' => TokenKind::Caret,
            '|' if self.current() == Some('|') => {
                self.advance();
                TokenKind::PipePipe
            }
            '|' => TokenKind::Pipe,
            '~' => TokenKind::Tilde,
            _ => {
                return Err(Diagnostic::new(
                    format!("unexpected character '{c}'"),
                    self.span(line, column),
                ))
            }
        };
        Ok(kind)
    }
}
