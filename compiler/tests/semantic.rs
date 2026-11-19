use jahe_compiler::{frontend, semantic};

fn errors(source: &str) -> Vec<semantic::SemanticDiagnostic> {
    frontend::analyze_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap()
}

#[test]
fn accepts_valid_typed_program_and_forward_calls() {
    let source = "module m; fn main() -> i32 { let x: i32 = add(1, 2); println(x); return x; } fn add(a: i32, b: i32) -> i32 { return a + b; }";
    assert!(errors(source).is_empty());
}

#[test]
fn rejects_unknown_types_and_duplicate_functions_or_parameters() {
    let result = errors("module m; fn f(a: nope, a: i32) {} fn f() {}");
    assert!(result.iter().any(|e| e.code == "E0101"));
    assert!(result.iter().any(|e| e.code == "E0201"));
    assert!(result.iter().any(|e| e.code == "E0202"));
}

#[test]
fn validates_signed_unsigned_and_pointer_integer_literal_ranges() {
    let source = r#"
        module integer_ranges;
        func a() -> i8 { return 128; }
        func b() -> i8 { return -129; }
        func c() -> i16 { return 32768; }
        func d() -> i16 { return -32769; }
        func e() -> i32 { return 2147483648; }
        func f() -> i32 { return -2147483649; }
        func g() -> i64 { return 9223372036854775808; }
        func h() -> i64 { return -9223372036854775809; }
        func i() -> i128 { return 170141183460469231731687303715884105728; }
        func j() -> i128 { return -170141183460469231731687303715884105729; }
        func k() -> u8 { return 256; }
        func l() -> u16 { return 65536; }
        func m() -> u32 { return 4294967296; }
        func n() -> u64 { return 18446744073709551616; }
        func o() -> u128 { return 340282366920938463463374607431768211456; }
        func p() -> isize { return 9223372036854775808; }
        func q() -> usize { return 18446744073709551616; }
        func r() -> u8 { return -1; }
    "#;
    let diagnostics = frontend::analyze(source).unwrap();
    assert!(diagnostics.len() >= 18, "diagnostics: {diagnostics:#?}");
    assert!(diagnostics
        .iter()
        .all(|d| d.code == "E0305" || d.code == "E0301"));
}

#[test]
fn rejects_duplicates_undefined_names_and_scope_escapes() {
    let result = errors("module m; fn f() { let x: i32 = 1; let x: i32 = 2; { let y: bool = true; } println(y); missing(); }");
    assert!(result.iter().any(|e| e.code == "E0203"));
    assert!(result
        .iter()
        .any(|e| e.code == "E0401" && e.message.contains("y")));
    assert!(result.iter().any(|e| e.code == "E0402"));
}

#[test]
fn rejects_type_mismatches_in_initializers_operators_and_returns() {
    let result =
        errors("module m; fn f() -> bool { let x: i32 = true; let y: i32 = 1 + false; return 4; }");
    assert!(result.iter().filter(|e| e.code == "E0301").count() >= 2);
    assert!(result
        .iter()
        .any(|e| e.message.contains("return") || e.expected.as_deref() == Some("bool")));
}

#[test]
fn checks_calls_and_comparisons() {
    let result = errors(
        "module m; fn take(x: f64) -> bool { return x > 1.0; } fn main() { take(true); take(); }",
    );
    assert!(result.iter().any(|e| e.code == "E0301"));
    assert!(result.iter().any(|e| e.code == "E0501"));
}

#[test]
fn nested_blocks_shadow_and_restore_lexical_scope() {
    let result = errors(
        "module m; fn f() { let x: i32 = 1; { let x: bool = true; println(x); } println(x); }",
    );
    assert!(result.is_empty());
}

#[test]
fn checks_mutability_and_boolean_control_flow_conditions() {
    let result = errors("module m; fn f() { let x: i32 = 0; x = 1; if 2 { println(x); } while true { let mut y: i32 = 0; y = y + 1; } }");
    assert!(result.iter().any(|e| e.code == "E0204"));
    assert!(result.iter().any(|e| e.expected.as_deref() == Some("bool")));
    assert!(
        errors("module m; fn f() { let mut x: i32 = 0; while x < 3 { x = x + 1; } }").is_empty()
    );
}

#[test]
fn checks_return_completeness_across_structured_control_flow() {
    let missing = errors("module m; fn f(flag: bool) -> i32 { if flag { return 1; } }");
    assert!(missing.iter().any(|e| e.code == "E0601"));
    assert!(errors(
        "module m; fn f(flag: bool) -> i32 { if flag { return 1; } else { return 2; } }"
    )
    .is_empty());
    assert!(errors("module m; fn f() -> i32 { { return 1; } }").is_empty());
}

#[test]
fn never_is_not_void_or_a_storable_parameter_value() {
    let errors =
        frontend::analyze("module never_types; func bad(value: never) -> never { return; }")
            .unwrap();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("parameter cannot have type `never`")));
    assert!(errors.iter().any(|e| e.message.contains("type mismatch")));
}

#[test]
fn character_ordering_is_rejected_until_the_language_rule_is_approved() {
    let errors = frontend::analyze(
        "module char_order; func less(a: char, b: char) -> bool { return a < b; }",
    )
    .unwrap();
    assert!(errors.iter().any(|e| e.code == "E0303"));
}

#[test]
fn cyclic_constant_dependencies_have_a_diagnostic() {
    let errors =
        frontend::analyze("module const_cycle; const A: i32 = B; const B: i32 = A;").unwrap();
    assert_eq!(errors.iter().filter(|e| e.code == "E0702").count(), 2);
}
