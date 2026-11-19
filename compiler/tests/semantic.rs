use jahe_compiler::{frontend, semantic};
mod common;

fn errors(source: &str) -> Vec<semantic::SemanticDiagnostic> {
    frontend::analyze_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap()
}

#[test]
fn scalar_operator_and_numeric_cast_matrix_is_semantically_valid() {
    let source = common::scalar_operator_cast_matrix();
    let diagnostics = frontend::analyze(&source).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn bool_and_char_cast_matrix_is_rejected_without_adding_cast_rules() {
    for ty in common::numeric_types() {
        for (source_ty, value) in [("bool", "true"), ("char", "'x'")] {
            let source = format!(
                "module bad_cast; func main() -> i32 {{ let value: {source_ty} = {value}; let casted: {ty} = value as {ty}; return 0; }}"
            );
            let diagnostics = errors(&source);
            assert!(
                diagnostics.iter().any(|item| item.code == "E0304"),
                "expected rejection for {source_ty} as {ty}: {diagnostics:#?}"
            );
        }
        for target_ty in ["bool", "char"] {
            let source = format!(
                "module bad_cast; func main() -> i32 {{ let value: {ty} = 1; let casted: {target_ty} = value as {target_ty}; return 0; }}"
            );
            let diagnostics = errors(&source);
            assert!(
                diagnostics.iter().any(|item| item.code == "E0304"),
                "expected rejection for {ty} as {target_ty}: {diagnostics:#?}"
            );
        }
    }
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

#[test]
fn typed_constant_evaluation_preserves_wide_integer_precision() {
    let source = r#"
        module typed_wide_constants;
        const UMAX: u128 = 340282366920938463463374607431768211455;
        const UADJ: u128 = UMAX - 1;
        const IMIN: i128 = -170141183460469231731687303715884105728;
        const INEXT: i128 = IMIN + 1;
        const PRODUCT: u128 = (18446744073709551615 * 18446744073709551615);
        const QUOTIENT: u128 = UMAX / 3;
        const REMAINDER: u128 = UMAX % 3;
        const SHIFTED: u128 = (1 << 127) | 7;
        const MAX_IS_GREATER: bool = UMAX > UADJ;
        func main() -> i32 {
            println(UADJ); println(IMIN); println(INEXT); println(PRODUCT);
            println(QUOTIENT); println(REMAINDER); println(SHIFTED); println(MAX_IS_GREATER);
            return 0;
        }
    "#;
    let diagnostics = errors(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let ir = frontend::lower_to_ir(source).unwrap();
    let result = jahe_compiler::backend::interpreter::Interpreter::default()
        .execute(&ir)
        .unwrap();
    assert_eq!(
        result.output.lines().collect::<Vec<_>>(),
        [
            "340282366920938463463374607431768211454",
            "-170141183460469231731687303715884105728",
            "-170141183460469231731687303715884105727",
            "340282366920938463426481119284349108225",
            "113427455640312821154458202477256070485",
            "0",
            "170141183460469231731687303715884105735",
            "true",
        ]
    );
}

#[test]
fn typed_constants_accept_integer_boundaries_for_all_integer_types() {
    let source = r#"
        module scalar_constant_boundaries;
        const I8_MIN: i8 = -128; const I8_MAX: i8 = 127;
        const I16_MIN: i16 = -32768; const I16_MAX: i16 = 32767;
        const I32_MIN: i32 = -2147483648; const I32_MAX: i32 = 2147483647;
        const I64_MIN: i64 = -9223372036854775808; const I64_MAX: i64 = 9223372036854775807;
        const I128_MIN: i128 = -170141183460469231731687303715884105728;
        const I128_MAX: i128 = 170141183460469231731687303715884105727;
        const ISIZE_MIN: isize = -9223372036854775808; const ISIZE_MAX: isize = 9223372036854775807;
        const U8_MAX: u8 = 255; const U16_MAX: u16 = 65535;
        const U32_MAX: u32 = 4294967295; const U64_MAX: u64 = 18446744073709551615;
        const U128_MAX: u128 = 340282366920938463463374607431768211455;
        const USIZE_MAX: usize = 18446744073709551615;
        func main() -> i32 { return 0; }
    "#;
    let diagnostics = errors(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn typed_constant_evaluation_reports_operation_specific_integer_errors() {
    for (ty, expression, code, phrase) in [
        ("i8", "127 + 1", "E0703", "overflow"),
        ("i8", "-128 - 1", "E0704", "underflow"),
        ("i8", "1 / 0", "E0705", "division by zero"),
        ("i8", "-128 / -1", "E0706", "negative one"),
        ("i8", "1 << 8", "E0707", "shift count"),
        ("u8", "255 << 1", "E0703", "overflow"),
        (
            "i128",
            "170141183460469231731687303715884105727 + 1",
            "E0703",
            "overflow",
        ),
        (
            "u128",
            "340282366920938463463374607431768211455 + 1",
            "E0703",
            "overflow",
        ),
        ("u8", "256 as u8", "E0708", "cast"),
        ("i8", "missing", "E0710", "dependency"),
        ("i8", "true + false", "E0709", "type"),
    ] {
        let source = format!(
            "module bad_const; const BAD: {ty} = {expression}; func main() -> i32 {{ return 0; }}"
        );
        let diagnostics = errors(&source);
        let diagnostic = diagnostics
            .iter()
            .find(|item| item.code == code)
            .unwrap_or_else(|| panic!("expected {code}, got {diagnostics:#?}"));
        assert!(diagnostic.message.contains(phrase), "{diagnostic:?}");
        assert!(diagnostic.span.line >= 1 && diagnostic.span.column >= 1);
    }
}

#[test]
fn signed_minimum_remainder_by_negative_one_is_zero_but_division_is_rejected() {
    for (ty, minimum) in [
        ("i8", "-128"),
        ("i16", "-32768"),
        ("i32", "-2147483648"),
        ("i64", "-9223372036854775808"),
        ("i128", "-170141183460469231731687303715884105728"),
        ("isize", "-9223372036854775808"),
    ] {
        let remainder = format!(
            "module rem_min; const MIN: {ty} = {minimum}; const REM: {ty} = MIN % -1; func main() -> i32 {{ if REM == 0 {{ return 0; }} return 1; }}"
        );
        assert!(
            frontend::analyze(&remainder).unwrap().is_empty(),
            "{remainder}"
        );
        let module = frontend::lower_to_ir(&remainder).unwrap();
        let result = jahe_compiler::backend::interpreter::Interpreter::default()
            .execute(&module)
            .unwrap();
        assert_eq!(
            result.return_value,
            Some(jahe_compiler::backend::interpreter::value::RuntimeValue::I32(0)),
            "{ty}"
        );

        let division = format!(
            "module div_min; const MIN: {ty} = {minimum}; const QUOTIENT: {ty} = MIN / -1; func main() -> i32 {{ return 0; }}"
        );
        let diagnostics = errors(&division);
        assert!(
            diagnostics.iter().any(|item| item.code == "E0706"),
            "expected signed division overflow for {ty}: {diagnostics:#?}"
        );
    }
}

#[test]
fn constant_runtime_error_categories_cover_each_integer_width() {
    for (ty, min, max, width, signed) in [
        ("i8", "-128", "127", 8, true),
        ("i16", "-32768", "32767", 16, true),
        ("i32", "-2147483648", "2147483647", 32, true),
        (
            "i64",
            "-9223372036854775808",
            "9223372036854775807",
            64,
            true,
        ),
        (
            "i128",
            "-170141183460469231731687303715884105728",
            "170141183460469231731687303715884105727",
            128,
            true,
        ),
        (
            "isize",
            "-9223372036854775808",
            "9223372036854775807",
            64,
            true,
        ),
        ("u8", "0", "255", 8, false),
        ("u16", "0", "65535", 16, false),
        ("u32", "0", "4294967295", 32, false),
        ("u64", "0", "18446744073709551615", 64, false),
        (
            "u128",
            "0",
            "340282366920938463463374607431768211455",
            128,
            false,
        ),
        ("usize", "0", "18446744073709551615", 64, false),
    ] {
        let mut cases = vec![
            ("overflow", format!("{max} + 1"), "E0703"),
            ("zero_div", "1 / 0".into(), "E0705"),
            ("zero_rem", "1 % 0".into(), "E0705"),
            ("bad_shift", format!("1 << {width}"), "E0707"),
        ];
        if signed {
            let signed_underflow_code = if width == 128 { "E0703" } else { "E0704" };
            cases.extend([
                ("underflow", format!("{min} - 1"), signed_underflow_code),
                (
                    "multiply_underflow",
                    format!("{min} * 2"),
                    signed_underflow_code,
                ),
                ("min_div_neg_one", format!("{min} / -1"), "E0706"),
                ("negate_min", format!("-{min}"), "E0703"),
            ]);
        } else {
            cases.extend([
                ("underflow", "0 - 1".into(), "E0704"),
                ("multiply_overflow", format!("{max} * 2"), "E0703"),
                ("negate_positive", "-1".into(), "E0704"),
            ]);
        }
        for (name, expression, code) in cases {
            let source = format!(
                "module const_boundary_error; const BAD: {ty} = {expression}; func main() -> i32 {{ return 0; }}"
            );
            let diagnostics = errors(&source);
            let diagnostic = diagnostics
                .iter()
                .find(|item| item.code == code)
                .unwrap_or_else(|| panic!("{ty} {name}: expected {code}, got {diagnostics:#?}"));
            assert!(diagnostic.span.line > 0 && diagnostic.span.column > 0);
        }
    }
}

#[test]
fn constant_integer_cast_range_errors_cover_every_integer_pair() {
    let types = [
        ("i8", 8, true),
        ("i16", 16, true),
        ("i32", 32, true),
        ("i64", 64, true),
        ("i128", 128, true),
        ("isize", 64, true),
        ("u8", 8, false),
        ("u16", 16, false),
        ("u32", 32, false),
        ("u64", 64, false),
        ("u128", 128, false),
        ("usize", 64, false),
    ];
    let signed_min = |width: u32| {
        if width == 128 {
            i128::MIN
        } else {
            -(1i128 << (width - 1))
        }
    };
    let signed_max = |width: u32| {
        if width == 128 {
            i128::MAX
        } else {
            (1i128 << (width - 1)) - 1
        }
    };
    let unsigned_max = |width: u32| {
        if width == 128 {
            u128::MAX
        } else {
            (1u128 << width) - 1
        }
    };
    let mut checked = 0usize;
    for (source_ty, source_width, source_signed) in types {
        for (target_ty, target_width, target_signed) in types {
            let mut invalid_values = Vec::new();
            if source_signed && target_signed {
                let source_min = signed_min(source_width);
                let source_max = signed_max(source_width);
                let target_min = signed_min(target_width);
                let target_max = signed_max(target_width);
                if source_min < target_min {
                    invalid_values.push((target_min - 1).to_string());
                }
                if source_max > target_max {
                    invalid_values.push((target_max + 1).to_string());
                }
            } else if source_signed {
                invalid_values.push(signed_min(source_width).to_string());
                let source_max = signed_max(source_width) as u128;
                let target_max = unsigned_max(target_width);
                if source_max > target_max {
                    invalid_values.push((target_max + 1).to_string());
                }
            } else {
                let source_max = unsigned_max(source_width);
                let target_max = if target_signed {
                    signed_max(target_width) as u128
                } else {
                    unsigned_max(target_width)
                };
                if source_max > target_max {
                    invalid_values.push((target_max + 1).to_string());
                }
            }
            for value in invalid_values {
                let source = format!(
                    "module bad_integer_cast; const SOURCE: {source_ty} = {value}; const BAD: {target_ty} = SOURCE as {target_ty}; func main() -> i32 {{ return 0; }}"
                );
                let diagnostics = errors(&source);
                let diagnostic = diagnostics.iter().find(|item| item.code == "E0708");
                assert!(
                    diagnostic.is_some_and(|item| {
                        item.span.line > 0
                            && item.span.column > 0
                            && item.message.contains("cast")
                            && item.message.contains(target_ty)
                    }),
                    "{source_ty} value {value} as {target_ty}: {diagnostics:#?}"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 100,
        "expected broad pairwise cast boundary coverage, got {checked}"
    );
}

#[test]
fn f32_constant_operations_round_after_each_operation() {
    let source = r#"
        module f32_rounding;
        const ROUNDED: f32 = (16777216.0 + 1.0) - 16777216.0;
        func main() -> i32 { println(ROUNDED); return 0; }
    "#;
    let ir = frontend::lower_to_ir(source).unwrap();
    let printed = jahe_compiler::ir::format(&ir);
    assert!(printed.contains("%0: f32 = const 0"), "{printed}");
    let result = jahe_compiler::backend::interpreter::Interpreter::default()
        .execute(&ir)
        .unwrap();
    assert_eq!(result.output.trim(), "0");
}

#[test]
fn typed_float_constants_preserve_ieee_edges_and_f64_precision() {
    let source = r#"
        module typed_float_constants;
        const NEG_ZERO: f64 = -0.0;
        const INF: f64 = 1.0 / 0.0;
        const NAN_NE: bool = (0.0 / 0.0) != (0.0 / 0.0);
        const PRECISE: f64 = 9007199254740992.0 + 2.0;
        func main() -> i32 {
            println(NEG_ZERO); println(INF); println(NAN_NE); println(PRECISE);
            return 0;
        }
    "#;
    let ir = frontend::lower_to_ir(source).unwrap();
    let result = jahe_compiler::backend::interpreter::Interpreter::default()
        .execute(&ir)
        .unwrap();
    let lines = result.output.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "-0");
    assert_eq!(lines[1], "inf");
    assert_eq!(lines[2], "true");
    assert_eq!(lines[3], "9007199254740994");
}
