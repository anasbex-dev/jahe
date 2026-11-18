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
    let module = frontend::parse_with_mode(
        "module m; fn main(x: i32) -> i32 { let y: i32 = x + 1; println(y); return y; }",
        jahe_compiler::parser::SyntaxMode::Legacy,
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
    let error = frontend::parse_with_mode(
        "module m;\nfn f() { let = 10; }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert_eq!((error.span.line, error.span.column), (2, 14));
    assert!(error.message.contains("identifier"));
}

#[test]
fn reports_lexical_errors() {
    let error = Lexer::new("module m;\n@ ").tokenize().unwrap_err();
    assert_eq!((error.span.line, error.span.column), (2, 1));
}

#[test]
fn canonical_mode_parses_func_let_var_inference_and_constants() {
    let module = frontend::parse(
        "module m; const ANSWER: i32 = 40 + 2; func main() -> i32 { let base = 40; var result = base + 2; return result; }",
    ).unwrap();
    assert_eq!(module.constants.len(), 1);
    assert!(matches!(
        module.functions[0].body[0],
        Statement::Let {
            ty: None,
            mutable: false,
            ..
        }
    ));
    assert!(matches!(
        module.functions[0].body[1],
        Statement::Let {
            ty: None,
            mutable: true,
            ..
        }
    ));
}

#[test]
fn canonical_mode_rejects_legacy_syntax_with_guidance() {
    let error = frontend::parse("module m; fn main() {}").unwrap_err();
    assert!(error.message.contains("--legacy-syntax"));
    let error = frontend::parse("module m; func main() { let mut value: i32 = 1; }").unwrap_err();
    assert!(error.message.contains("--legacy-syntax"));
    assert!(frontend::parse_with_mode(
        "module m; fn main() {}",
        jahe_compiler::parser::SyntaxMode::Legacy
    )
    .is_ok());
}

#[test]
fn generic_dot_chain_and_tuple_projection_have_distinct_ast_nodes() {
    let module =
        frontend::parse("module m; func f() { factory().profile.email; tuple.0; }").unwrap();
    let Statement::Expression(chain, _) = &module.functions[0].body[0] else {
        panic!("expected expression")
    };
    assert!(matches!(chain.kind, ExprKind::Member { .. }));
    let Statement::Expression(tuple, _) = &module.functions[0].body[1] else {
        panic!("expected expression")
    };
    assert!(matches!(tuple.kind, ExprKind::TupleIndex { index: 0, .. }));
}

#[test]
fn numeric_separators_are_checked_and_removed_from_tokens() {
    let tokens = Lexer::new("module m; func f() { let x: i32 = 1_000; }")
        .tokenize()
        .unwrap();
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Integer(1000)));
    assert!(Lexer::new("module m; func f() { let x: i32 = 1__0; }")
        .tokenize()
        .is_err());
    assert!(Lexer::new("module m; func f() { let x: i32 = 10_; }")
        .tokenize()
        .is_err());
}
