pub mod ast;
use crate::{
    diagnostics::Diagnostic,
    lexer::{
        token::{Keyword, Token, TokenKind},
        Lexer,
    },
};
use ast::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyntaxMode {
    #[default]
    Canonical,
    Legacy,
}

pub fn parse(source: &str) -> Result<Module, Diagnostic> {
    parse_with_mode(source, SyntaxMode::Canonical)
}

pub fn parse_with_mode(source: &str, mode: SyntaxMode) -> Result<Module, Diagnostic> {
    let tokens = Lexer::new(source).tokenize()?;
    Parser {
        tokens,
        current: 0,
        mode,
    }
    .module()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    mode: SyntaxMode,
}
impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }
    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.current += 1;
        }
        token
    }
    fn at(&self, kind: &TokenKind) -> bool {
        match (&self.peek().kind, kind) {
            (TokenKind::Keyword(a), TokenKind::Keyword(b)) => a == b,
            _ => std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind),
        }
    }
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, kind: TokenKind, message: &str) -> Result<Token, Diagnostic> {
        if self.at(&kind) {
            Ok(self.advance())
        } else {
            Err(Diagnostic::new(message, self.peek().span))
        }
    }
    fn identifier(&mut self, message: &str) -> Result<String, Diagnostic> {
        match self.advance() {
            Token {
                kind: TokenKind::Identifier(value),
                ..
            } => Ok(value),
            token => Err(Diagnostic::new(message, token.span)),
        }
    }
    fn module(mut self) -> Result<Module, Diagnostic> {
        self.expect(
            TokenKind::Keyword(Keyword::Module),
            "expected module declaration",
        )?;
        let name = self.identifier("expected module name")?;
        self.expect(
            TokenKind::Semicolon,
            "expected ';' after module declaration",
        )?;
        let mut constants = Vec::new();
        let mut functions = Vec::new();
        while !self.at(&TokenKind::Eof) {
            if self.at(&TokenKind::Keyword(Keyword::Const)) {
                constants.push(self.constant()?);
            } else {
                functions.push(self.function()?);
            }
        }
        Ok(Module {
            name,
            constants,
            functions,
        })
    }
    fn function(&mut self) -> Result<Function, Diagnostic> {
        let token = self.advance();
        let span = token.span;
        match token.kind {
            TokenKind::Keyword(Keyword::Func) => {}
            TokenKind::Keyword(Keyword::Fn) if self.mode == SyntaxMode::Legacy => {}
            TokenKind::Keyword(Keyword::Fn) => {
                return Err(Diagnostic::new(
                    "legacy `fn` syntax requires --legacy-syntax; use `func` in JAHE 0.8",
                    span,
                ))
            }
            _ => {
                return Err(Diagnostic::new(
                    "expected function declaration starting with `func`",
                    span,
                ))
            }
        }
        let name = self.identifier("expected function name")?;
        self.expect(TokenKind::LeftParen, "expected '(' after function name")?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RightParen) {
            loop {
                let token = self.peek().clone();
                let pname = self.identifier("expected parameter name")?;
                self.expect(TokenKind::Colon, "expected ':' after parameter name")?;
                params.push(Parameter {
                    name: pname,
                    span: token.span,
                    ty: self.ty()?,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                if self.at(&TokenKind::RightParen) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RightParen, "expected ')' after parameters")?;
        let return_type = if self.eat(&TokenKind::Arrow) {
            Some(self.ty()?)
        } else {
            None
        };
        let body = self.block_body()?;
        Ok(Function {
            name,
            span,
            params,
            return_type,
            body,
        })
    }
    fn constant(&mut self) -> Result<Constant, Diagnostic> {
        let span = self.advance().span;
        let name = self.identifier("expected constant name")?;
        self.expect(TokenKind::Colon, "constant type annotation is required")?;
        let ty = self.ty()?;
        self.expect(TokenKind::Equal, "expected '=' in constant declaration")?;
        let value = self.expression()?;
        self.expect(
            TokenKind::Semicolon,
            "expected ';' after constant declaration",
        )?;
        Ok(Constant {
            name,
            ty,
            value,
            span,
        })
    }
    fn ty(&mut self) -> Result<Type, Diagnostic> {
        Ok(Type(self.identifier("expected type name")?))
    }
    fn block_body(&mut self) -> Result<Vec<Statement>, Diagnostic> {
        self.expect(TokenKind::LeftBrace, "expected '{'")?;
        let mut body = Vec::new();
        while !self.at(&TokenKind::RightBrace) && !self.at(&TokenKind::Eof) {
            body.push(self.statement()?);
        }
        self.expect(TokenKind::RightBrace, "expected '}' after block")?;
        Ok(body)
    }
    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        if self.at(&TokenKind::LeftBrace) {
            let span = self.advance().span;
            let mut body = Vec::new();
            while !self.at(&TokenKind::RightBrace) && !self.at(&TokenKind::Eof) {
                body.push(self.statement()?);
            }
            self.expect(TokenKind::RightBrace, "expected '}' after block")?;
            return Ok(Statement::Block(body, span));
        }
        if self.at(&TokenKind::Keyword(Keyword::Const)) {
            return self.constant().map(Statement::Const);
        }
        if self.at(&TokenKind::Keyword(Keyword::Let)) || self.at(&TokenKind::Keyword(Keyword::Var))
        {
            let declaration = self.advance();
            let mutable = matches!(declaration.kind, TokenKind::Keyword(Keyword::Var));
            let old_mut = self.eat(&TokenKind::Keyword(Keyword::Mut));
            if old_mut && self.mode != SyntaxMode::Legacy {
                return Err(Diagnostic::new(
                    "legacy `let mut` requires --legacy-syntax; use `var`",
                    declaration.span,
                ));
            }
            if old_mut && mutable {
                return Err(Diagnostic::new(
                    "`var mut` is invalid; use `var name`",
                    declaration.span,
                ));
            }
            let name = self.identifier("expected identifier after binding keyword")?;
            let ty = if self.eat(&TokenKind::Colon) {
                Some(self.ty()?)
            } else {
                None
            };
            self.expect(TokenKind::Equal, "expected '=' in variable declaration")?;
            let value = self.expression()?;
            self.expect(
                TokenKind::Semicolon,
                "expected ';' after variable declaration",
            )?;
            return Ok(Statement::Let {
                name,
                ty,
                value,
                mutable: mutable || old_mut,
                span: declaration.span,
            });
        }
        if self.at(&TokenKind::Keyword(Keyword::If)) {
            let span = self.advance().span;
            let condition = self.expression()?;
            let then_body = self.block_body()?;
            let else_body = if self.eat(&TokenKind::Keyword(Keyword::Else)) {
                if self.at(&TokenKind::Keyword(Keyword::If)) {
                    vec![self.statement()?]
                } else {
                    self.block_body()?
                }
            } else {
                Vec::new()
            };
            return Ok(Statement::If {
                condition,
                then_body,
                else_body,
                span,
            });
        }
        if self.at(&TokenKind::Keyword(Keyword::While)) {
            let span = self.advance().span;
            let condition = self.expression()?;
            let body = self.block_body()?;
            return Ok(Statement::While {
                condition,
                body,
                span,
            });
        }
        if self.at(&TokenKind::Keyword(Keyword::Return)) {
            let span = self.advance().span;
            let value = if self.at(&TokenKind::Semicolon) {
                None
            } else {
                Some(self.expression()?)
            };
            self.expect(TokenKind::Semicolon, "expected ';' after return")?;
            return Ok(Statement::Return(value, span));
        }
        let span = self.peek().span;
        let expr = self.expression()?;
        if self.eat(&TokenKind::Equal) {
            let name = match expr.kind {
                ExprKind::Name(name) => name,
                _ => {
                    return Err(Diagnostic::new(
                        "assignment target must be a local variable",
                        expr.span,
                    ))
                }
            };
            let value = self.expression()?;
            self.expect(TokenKind::Semicolon, "expected ';' after assignment")?;
            return Ok(Statement::Assign { name, value, span });
        }
        self.expect(TokenKind::Semicolon, "expected ';' after expression")?;
        Ok(Statement::Expression(expr, span))
    }
    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        self.binary(0)
    }
    fn binary(&mut self, min: u8) -> Result<Expr, Diagnostic> {
        let mut left = self.postfix()?;
        loop {
            let (prec, op) = match self.peek().kind {
                TokenKind::PipePipe => (0, "||"),
                TokenKind::AmpAmp => (1, "&&"),
                TokenKind::EqualEqual => (2, "=="),
                TokenKind::BangEqual => (2, "!="),
                TokenKind::Less => (2, "<"),
                TokenKind::LessEqual => (2, "<="),
                TokenKind::Greater => (2, ">"),
                TokenKind::GreaterEqual => (2, ">="),
                TokenKind::Pipe => (3, "|"),
                TokenKind::Caret => (4, "^"),
                TokenKind::Ampersand => (5, "&"),
                TokenKind::ShiftLeft => (6, "<<"),
                TokenKind::ShiftRight => (6, ">>"),
                TokenKind::Plus => (7, "+"),
                TokenKind::Minus => (7, "-"),
                TokenKind::Star => (8, "*"),
                TokenKind::Slash => (8, "/"),
                TokenKind::Percent => (8, "%"),
                _ => break,
            };
            if prec < min {
                break;
            }
            self.advance();
            let right = self.binary(prec + 1)?;
            let span = left.span;
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: op.into(),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }
    fn postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.primary()?;
        loop {
            if self.eat(&TokenKind::LeftParen) {
                let mut args = Vec::new();
                if !self.at(&TokenKind::RightParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                        if self.at(&TokenKind::RightParen) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RightParen, "expected ')' after arguments")?;
                let span = expr.span;
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
            } else if self.eat(&TokenKind::Dot) {
                let token = self.advance();
                let kind = match token.kind {
                    TokenKind::Identifier(member) => ExprKind::Member {
                        base: Box::new(expr.clone()),
                        member,
                    },
                    TokenKind::Integer(index) => {
                        let digits = index.raw.replace('_', "");
                        let Ok(index) = digits.parse::<u32>() else {
                            return Err(Diagnostic::new("tuple index is out of range", token.span));
                        };
                        ExprKind::TupleIndex {
                            base: Box::new(expr.clone()),
                            index,
                        }
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            "expected member name or tuple position after '.'",
                            token.span,
                        ))
                    }
                };
                expr = Expr {
                    kind,
                    span: expr.span,
                };
            } else if self.eat(&TokenKind::Keyword(Keyword::As)) {
                let target = self.ty()?;
                let span = expr.span;
                expr = Expr {
                    kind: ExprKind::Cast {
                        value: Box::new(expr),
                        target,
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }
    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.advance();
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Integer(v) => ExprKind::Integer(v),
            TokenKind::Float(v) => ExprKind::Float(v),
            TokenKind::Character(v) => ExprKind::Character(v),
            TokenKind::String(v) => ExprKind::String(v),
            TokenKind::Keyword(Keyword::True) => ExprKind::Bool(true),
            TokenKind::Keyword(Keyword::False) => ExprKind::Bool(false),
            TokenKind::Identifier(name) => ExprKind::Name(name),
            TokenKind::Minus => {
                let value = self.primary()?;
                return Ok(Expr {
                    kind: ExprKind::Unary {
                        op: "-".into(),
                        value: Box::new(value),
                    },
                    span,
                });
            }
            TokenKind::Bang => {
                let value = self.primary()?;
                return Ok(Expr {
                    kind: ExprKind::Unary {
                        op: "!".into(),
                        value: Box::new(value),
                    },
                    span,
                });
            }
            TokenKind::LeftParen => {
                let expr = self.expression()?;
                self.expect(TokenKind::RightParen, "expected ')' after expression")?;
                return Ok(expr);
            }
            _ => return Err(Diagnostic::new("expected expression", span)),
        };
        Ok(Expr { kind, span })
    }
}
