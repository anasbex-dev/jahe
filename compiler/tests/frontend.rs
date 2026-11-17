use jahe_compiler::{
    frontend,
    lexer::{
        token::{Keyword, TokenKind},
        Lexer,
    },
    parser::ast::{ExprKind, Statement},
};

#[test]
fn lexes_keywords_identifiers_numbers_strings_and_operators() {
    let tokens =
        Lexer::new("module abc; fn f(x: i32) -> f64 { let y = 12 + 3.5; return \"ok\\n\"; }")
            .tokenize()
            .unwrap();
    assert_eq!(tokens[0].kind, TokenKind::Keyword(Keyword::Module));
    assert_eq!(tokens[1].kind, TokenKind::Identifier("abc".into()));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Integer(12)));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Float(3.5)));
    assert!(tokens
        .iter()
        .any(|t| t.kind == TokenKind::String("ok\n".into())));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Plus));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Arrow));
}

#[test]
fn tracks_source_locations() {
    let tokens = Lexer::new("module x;\nfn f() {}").tokenize().unwrap();
    let function = tokens
        .iter()
        .find(|t| t.kind == TokenKind::Keyword(Keyword::Fn))
        .unwrap();
    assert_eq!((function.span.line, function.span.column), (2, 1));
}

#[test]
fn parses_functions_variables_calls_and_returns() {
    let module = frontend::parse(
        "module m; fn main(x: i32) -> i32 { let y: i32 = x + 1; println(y); return y; }",
    )
    .unwrap();
    assert_eq!(module.name, "m");
    let function = &module.functions[0];
    assert_eq!(function.name, "main");
    assert_eq!(function.params.len(), 1);
    assert!(matches!(function.body[0], Statement::Let { .. }));
    assert!(matches!(
        function.body[1],
        Statement::Expression(
            jahe_compiler::parser::ast::Expr {
                kind: ExprKind::Call { .. },
                ..
            },
            _
        )
    ));
    assert!(matches!(function.body[2], Statement::Return(Some(_), _)));
}

#[test]
fn rejects_invalid_syntax_with_location() {
    let error = frontend::parse("module m;\nfn f() { let = 10; }").unwrap_err();
    assert_eq!((error.span.line, error.span.column), (2, 14));
    assert!(error.message.contains("identifier"));
}

#[test]
fn reports_lexical_errors() {
    let error = Lexer::new("module m;\n@ ").tokenize().unwrap_err();
    assert_eq!((error.span.line, error.span.column), (2, 1));
}
