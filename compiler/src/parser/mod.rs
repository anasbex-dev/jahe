pub mod ast;
use crate::{
    diagnostics::Diagnostic,
    lexer::{
        token::{Keyword, Token, TokenKind},
        Lexer,
    },
};
use ast::*;

pub fn parse(source: &str) -> Result<Module, Diagnostic> {
    let tokens = Lexer::new(source).tokenize()?;
    Parser { tokens, current: 0 }.module()
}
struct Parser {
    tokens: Vec<Token>,
    current: usize,
}
impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }
    fn advance(&mut self) -> Token {
        let t = self.peek().clone();
        if !matches!(t.kind, TokenKind::Eof) {
            self.current += 1;
        }
        t
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
                kind: TokenKind::Identifier(v),
                ..
            } => Ok(v),
            t => Err(Diagnostic::new(message, t.span)),
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
        let mut functions = Vec::new();
        while !self.at(&TokenKind::Eof) {
            functions.push(self.function()?);
        }
        Ok(Module { name, functions })
    }
    fn function(&mut self) -> Result<Function, Diagnostic> {
        let span = self
            .expect(
                TokenKind::Keyword(Keyword::Fn),
                "expected function declaration",
            )?
            .span;
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
        if self.at(&TokenKind::Keyword(Keyword::Let)) {
            let span = self.advance().span;
            let mutable = self.eat(&TokenKind::Keyword(Keyword::Mut));
            let name = self.identifier("expected identifier after 'let'")?;
            self.expect(TokenKind::Colon, "expected ':' after variable name")?;
            let ty = self.ty()?;
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
                mutable,
                span,
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
        if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            let span = self.peek().span;
            if self
                .tokens
                .get(self.current + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::Equal))
            {
                self.advance();
                self.advance();
                let value = self.expression()?;
                self.expect(TokenKind::Semicolon, "expected ';' after assignment")?;
                return Ok(Statement::Assign { name, value, span });
            }
        }
        let span = self.peek().span;
        let expr = self.expression()?;
        self.expect(TokenKind::Semicolon, "expected ';' after expression")?;
        Ok(Statement::Expression(expr, span))
    }
    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        self.binary(0)
    }
    fn binary(&mut self, min: u8) -> Result<Expr, Diagnostic> {
        let mut left = self.primary()?;
        loop {
            let (prec, op) = match self.peek().kind {
                TokenKind::EqualEqual => (1, "=="),
                TokenKind::BangEqual => (1, "!="),
                TokenKind::Less => (1, "<"),
                TokenKind::LessEqual => (1, "<="),
                TokenKind::Greater => (1, ">"),
                TokenKind::GreaterEqual => (1, ">="),
                TokenKind::Plus => (2, "+"),
                TokenKind::Minus => (2, "-"),
                TokenKind::Star => (3, "*"),
                TokenKind::Slash => (3, "/"),
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
    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.advance();
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Integer(v) => ExprKind::Integer(v),
            TokenKind::Float(v) => ExprKind::Float(v),
            TokenKind::String(v) => ExprKind::String(v),
            TokenKind::Keyword(Keyword::True) => ExprKind::Bool(true),
            TokenKind::Keyword(Keyword::False) => ExprKind::Bool(false),
            TokenKind::Identifier(name) if self.eat(&TokenKind::LeftParen) => {
                let mut args = Vec::new();
                if !self.at(&TokenKind::RightParen) {
                    loop {
                        args.push(self.expression()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RightParen, "expected ')' after arguments")?;
                ExprKind::Call { callee: name, args }
            }
            TokenKind::Identifier(name) => ExprKind::Name(name),
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
