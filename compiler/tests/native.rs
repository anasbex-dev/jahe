use jahe_compiler::{
    backend::{
        interpreter::{value::RuntimeValue, Interpreter},
        native::NativeBackend,
        BackendErrorKind,
    },
    frontend,
    ir::{self, ExternalFunction, InstructionKind, Signature, Type},
};
use std::{fs, process::Command};
mod common;

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_in_range_integer_narrowing_cast_does_not_trap() {
    let source = r#"module narrow_cast_regression;
        const SOURCE: i16 = 6;
        const EXPECTED: i8 = SOURCE as i8;
        func main() -> i32 {
            let source: i16 = SOURCE;
            if (source as i8) != EXPECTED { return 1; }
            return 0;
        }
    "#;
    let output = common::run_native_case("i16_to_i8_in_range", source).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_i128_zero_product_does_not_look_like_overflow() {
    let source = r#"module i128_zero_multiply;
        const ZERO: i128 = 0;
        const FACTOR: i128 = 123456789;
        const EXPECTED: i128 = ZERO * FACTOR;
        func main() -> i32 {
            let product: i128 = ZERO * FACTOR;
            if product == EXPECTED { return 0; }
            return 1;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let interpreted = Interpreter::default().execute(&module).unwrap();
    assert_eq!(interpreted.return_value, Some(RuntimeValue::I32(0)));
    let output = common::run_native_case("i128_zero_product", source).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_i128_u128_multiply_overflow_traps() {
    for (name, ty, max) in [
        ("i128", "i128", "170141183460469231731687303715884105727"),
        ("u128", "u128", "340282366920938463463374607431768211455"),
    ] {
        let source = format!(
            "module multiply_overflow; const MAX: {ty} = {max}; const TWO: {ty} = 2; func main() -> i32 {{ let overflow: {ty} = MAX * TWO; return 0; }}"
        );
        let module = frontend::lower_to_ir(&source).unwrap();
        assert!(
            Interpreter::default().execute(&module).is_err(),
            "{name} multiplication overflow unexpectedly passed in the interpreter"
        );
        let output =
            common::run_native_case(&format!("{name}_multiply_overflow"), &source).unwrap();
        assert_ne!(
            output.status.code(),
            Some(0),
            "{name} multiplication overflow returned normally"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_runtime_f32_rounds_after_each_operation() {
    let source = r#"module f32_runtime_rounding;
        func main() -> i32 {
            let large: f32 = 16777216.0 as f32;
            let one: f32 = 1.0 as f32;
            let rounded: f32 = (large + one) - large;
            let zero: f32 = 0.0 as f32;
            if rounded == zero { return 0; }
            return 1;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let interpreted = Interpreter::default().execute(&module).unwrap();
    assert_eq!(interpreted.return_value, Some(RuntimeValue::I32(0)));
    let output = common::run_native_case("f32_stepwise_runtime_rounding", source).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_float_to_i32_boundaries_match_interpreter_traps() {
    for (name, value, expected) in [
        ("signed_min", "-2147483648.0", Some(-2147483648)),
        ("signed_max", "2147483647.0", Some(2147483647)),
        ("above_max", "2147483648.0", None),
        ("nan", "0.0 / 0.0", None),
        ("positive_infinity", "1.0 / 0.0", None),
        ("negative_infinity", "-1.0 / 0.0", None),
    ] {
        let body = if let Some(expected) = expected {
            format!("if converted == {expected} {{ return 0; }} return 1;")
        } else {
            "return 0;".into()
        };
        let source = format!(
            "module float_to_i32_boundary; func main() -> i32 {{ let value: f64 = {value}; let converted: i32 = value as i32; {body} }}"
        );
        let module = frontend::lower_to_ir(&source).unwrap();
        let interpreted = Interpreter::default().execute(&module);
        assert_eq!(
            interpreted.is_ok(),
            expected.is_some(),
            "{name}: interpreter {interpreted:?}"
        );
        let native = common::run_native_case(&format!("float_to_i32_{name}"), &source).unwrap();
        assert_eq!(
            native.status.success(),
            expected.is_some(),
            "{name}: {native:?}"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_supported_scalar_operators_and_casts_run_in_isolated_processes() {
    const NUMERIC: [&str; 14] = [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64",
    ];
    let mut cases = Vec::<(String, String)>::new();
    for ty in NUMERIC {
        let float = matches!(ty, "f32" | "f64");
        let (a, b, one) = if float {
            ("6.0", "3.0", "1.0")
        } else {
            ("6", "3", "1")
        };
        let lhs = format!("const LEFT: {ty} = {a};");
        let rhs = format!("const RIGHT: {ty} = {b};");
        let binary_ops: &[&str] = if float {
            &["+", "-", "*", "/"]
        } else {
            &["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>"]
        };
        for op in binary_ops {
            let rhs_expr = if matches!(*op, "<<" | ">>") {
                format!("const RIGHT: {ty} = {one};")
            } else {
                rhs.clone()
            };
            let source = format!(
                "module native_isolated_case; {lhs} {rhs_expr} const EXPECTED: {ty} = LEFT {op} RIGHT; func main() -> i32 {{ let lhs: {ty} = LEFT; let rhs: {ty} = RIGHT; if ((lhs {op} rhs) != EXPECTED) {{ return 1; }} return 0; }}"
            );
            cases.push((format!("{ty}_{op}"), source));
        }
        for op in ["==", "!=", "<", "<=", ">", ">="] {
            let source = format!(
                "module native_isolated_case; {lhs} {rhs} const EXPECTED: bool = LEFT {op} RIGHT; func main() -> i32 {{ let lhs: {ty} = LEFT; let rhs: {ty} = RIGHT; if ((lhs {op} rhs) != EXPECTED) {{ return 1; }} return 0; }}"
            );
            cases.push((format!("{ty}_cmp_{op}"), source));
        }
        let (neg_input, neg_ty) = if ty.starts_with('u') || ty == "usize" {
            ("0", ty)
        } else {
            (a, ty)
        };
        let neg_source = format!(
            "module native_isolated_case; const VALUE: {ty} = {neg_input}; const EXPECTED: {neg_ty} = -VALUE; func main() -> i32 {{ let value: {ty} = VALUE; if ((-value) != EXPECTED) {{ return 1; }} return 0; }}"
        );
        cases.push((format!("{ty}_neg"), neg_source));

        for target in NUMERIC {
            if (matches!(ty, "i128" | "u128") && matches!(target, "f32" | "f64"))
                || (matches!(ty, "f32" | "f64") && matches!(target, "i128" | "u128"))
            {
                continue;
            }
            let cast_source = format!(
                "module native_isolated_case; const VALUE: {ty} = {a}; const EXPECTED: {target} = VALUE as {target}; func main() -> i32 {{ let value: {ty} = VALUE; if ((value as {target}) != EXPECTED) {{ return 1; }} return 0; }}"
            );
            cases.push((format!("cast_{ty}_to_{target}"), cast_source));
        }
    }

    // Each child executable is isolated, and a small fixed worker pool keeps the suite practical
    // without sharing generated files or linker outputs between cases.
    let mut workers = (0..8).map(|_| Vec::new()).collect::<Vec<Vec<_>>>();
    let worker_count = workers.len();
    for (index, (name, source)) in cases.into_iter().enumerate() {
        workers[index % worker_count].push((index, name, source));
    }
    let worker_results = std::thread::scope(|scope| {
        workers
            .into_iter()
            .map(|worker| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for (index, name, source) in worker {
                        let case_name = format!("{index}_{name}");
                        let module = match frontend::lower_to_ir(&source) {
                            Ok(module) => module,
                            Err(error) => {
                                failures.push(format!("{case_name}: frontend: {error:?}"));
                                continue;
                            }
                        };
                        let _verified = match ir::verify::Verifier.verify(&module) {
                            Ok(module) => module,
                            Err(error) => {
                                failures.push(format!("{case_name}: IR verification: {error:?}"));
                                continue;
                            }
                        };
                        match Interpreter::default().execute(&module) {
                            Ok(result) if result.return_value == Some(RuntimeValue::I32(0)) => {}
                            Ok(result) => failures.push(format!(
                                "{case_name}: interpreter returned {:?}",
                                result.return_value
                            )),
                            Err(error) => {
                                failures.push(format!("{case_name}: interpreter: {error}"));
                                continue;
                            }
                        }
                        match common::run_native_case(&case_name, &source) {
                            Ok(output) if output.status.code() == Some(0) => {}
                            Ok(output) => failures.push(format!(
                                "{case_name}: native exit={:?}, stdout={:?}, stderr={:?}",
                                output.status.code(),
                                String::from_utf8_lossy(&output.stdout),
                                String::from_utf8_lossy(&output.stderr)
                            )),
                            Err(error) => failures.push(format!("{case_name}: {error}")),
                        }
                    }
                    failures
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|handle| handle.join().expect("native case worker panicked"))
            .collect::<Vec<_>>()
    });
    if !worker_results.is_empty() {
        panic!(
            "isolated native/interpreter failures ({}):\n{}",
            worker_results.len(),
            worker_results.join("\n")
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_boolean_and_character_comparisons_run_in_isolated_processes() {
    let cases = [
        (
            "bool_not",
            r#"module bool_not_case; const A: bool = true; const EXPECTED: bool = !A;
               func main() -> i32 { let a: bool = A; if ((!a) != EXPECTED) { return 1; } return 0; }"#,
        ),
        (
            "bool_equal",
            r#"module bool_equal_case; const A: bool = true; const B: bool = false; const EXPECTED: bool = A == B;
               func main() -> i32 { let a: bool = A; let b: bool = B; if ((a == b) != EXPECTED) { return 1; } return 0; }"#,
        ),
        (
            "bool_not_equal",
            r#"module bool_not_equal_case; const A: bool = true; const B: bool = false; const EXPECTED: bool = A != B;
               func main() -> i32 { let a: bool = A; let b: bool = B; if ((a != b) != EXPECTED) { return 1; } return 0; }"#,
        ),
        (
            "char_equal",
            r#"module char_equal_case; const A: char = 'é'; const B: char = '\u{E9}'; const EXPECTED: bool = A == B;
               func main() -> i32 { let a: char = A; let b: char = B; if ((a == b) != EXPECTED) { return 1; } return 0; }"#,
        ),
        (
            "char_not_equal",
            r#"module char_not_equal_case; const A: char = 'é'; const B: char = 'z'; const EXPECTED: bool = A != B;
               func main() -> i32 { let a: char = A; let b: char = B; if ((a != b) != EXPECTED) { return 1; } return 0; }"#,
        ),
    ];
    for (name, source) in cases {
        let module = frontend::lower_to_ir(source).unwrap();
        let interpreted = Interpreter::default().execute(&module).unwrap();
        assert_eq!(
            interpreted.return_value,
            Some(RuntimeValue::I32(0)),
            "{name}"
        );
        let native = common::run_native_case(name, source).unwrap();
        assert_eq!(native.status.code(), Some(0), "{name}: {native:?}");
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_scalar_operator_and_cast_smoke_test() {
    let source = r#"module native_scalar_matrix;
        const SUM: i32 = 6 + 3;
        const DIFFERENCE: i32 = 6 - 3;
        const PRODUCT: i32 = 6 * 3;
        const QUOTIENT: i32 = 6 / 3;
        const REMAINDER: i32 = 7 % 3;
        const SHIFTED: i32 = 6 << 1;
        const CASTED: u32 = 6 as u32;
        const FLOAT_SUM: f32 = (1.5 as f32) + (2.5 as f32);
        const FLOAT_CAST: i32 = 6.0 as i32;
        func main() -> i32 {
            if (6 + 3) != SUM { return 1; }
            if (6 - 3) != DIFFERENCE { return 2; }
            if (6 * 3) != PRODUCT { return 3; }
            if (6 / 3) != QUOTIENT { return 4; }
            if (7 % 3) != REMAINDER { return 5; }
            if (6 << 1) != SHIFTED { return 6; }
            if ((6 as u32) != CASTED) { return 7; }
            if (((1.5 as f32) + (2.5 as f32)) != FLOAT_SUM) { return 8; }
            if ((6.0 as i32) != FLOAT_CAST) { return 9; }
            if !(6 < 7) { return 10; }
            return 0;
        }
    "#;
    let module = frontend::lower_to_ir(&source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root =
        std::env::temp_dir().join(format!("jahe_native_scalar_matrix_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("scalar_matrix.obj");
    let executable = root.join("scalar_matrix.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap_or_else(|error| panic!("native matrix compile failed: {error}"));
    assert!(object.is_file());
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_rejects_wide_integer_float_casts_with_clear_diagnostics() {
    for (source_ty, target_ty, value) in [
        ("i128", "f32", "1"),
        ("i128", "f64", "1"),
        ("u128", "f32", "1"),
        ("u128", "f64", "1"),
        ("f32", "i128", "1.0"),
        ("f32", "u128", "1.0"),
        ("f64", "i128", "1.0"),
        ("f64", "u128", "1.0"),
    ] {
        let source = format!(
            "module unsupported_cast; func main() -> i32 {{ let value: {source_ty} = {value}; let converted: {target_ty} = value as {target_ty}; if converted == converted {{ return 0; }} return 1; }}"
        );
        let module = frontend::lower_to_ir(&source).unwrap();
        let verified = ir::verify::Verifier.verify(&module).unwrap();
        let error = NativeBackend::default().emit_object(&verified).unwrap_err();
        assert!(
            error.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "NAT0002"
                    && diagnostic.message.contains(source_ty)
                    && diagnostic.message.contains(target_ty)
                    && diagnostic.message.contains("not implemented")
            }),
            "{source_ty} as {target_ty}: {error}"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_string_comparison_is_rejected_with_a_clear_diagnostic() {
    let source = r#"module unsupported_string_compare;
        func main() -> i32 {
            if "jahe" == "jahe" { return 0; }
            return 1;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let error = NativeBackend::default().emit_object(&verified).unwrap_err();
    assert!(
        error.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "NAT0002"
                && diagnostic.message.contains("string")
                && diagnostic.message.contains("comparison")
        }),
        "{error}"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_character_values_compare_and_execute() {
    let source = "module native_chars; func same(a: char, b: char) -> bool { return a == b; } func main() -> i32 { println('é'); if same('é', '\\u{E9}') { return 0; } else { return 1; } }";
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_char_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("char.obj");
    let executable = root.join("char.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file());
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "é\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_top_level_and_local_constant_references_execute() {
    let source = r#"module native_const_refs;
        const BASE: i32 = 40;
        const ANSWER: i32 = BASE + 2;
        func main() -> i32 {
            const LOCAL: i32 = ANSWER + 1;
            println(LOCAL);
            return LOCAL - 43;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_const_refs_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("const_refs.obj");
    let executable = root.join("const_refs.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "43\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_typed_wide_and_f32_constants_execute_with_expected_precision() {
    let source = r#"module native_typed_constants;
        const UMAX: u128 = 340282366920938463463374607431768211455;
        const UMINUSONE: u128 = UMAX - 1;
        const ROUNDED: f32 = (16777216.0 + 1.0) - 16777216.0;
        func main() -> i32 {
            if UMINUSONE + 1 == UMAX { println(ROUNDED); return 0; }
            return 1;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root =
        std::env::temp_dir().join(format!("jahe_native_typed_consts_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("typed_constants.obj");
    let executable = root.join("typed_constants.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file());
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "0\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_float_negation_and_ieee_division_execute() {
    let source = "module native_ieee; func negate(value: f64) -> f64 { return -value; } func main() -> i32 { let neg_zero = negate(0.0); let infinity = 1.0 / neg_zero; let nan = infinity / infinity; if nan != nan { if neg_zero == 0.0 { if infinity < 0.0 { return 0; } } } return 1; }";
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_ieee_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("ieee.obj");
    let executable = root.join("ieee.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_bool_logical_operators_short_circuit() {
    let source = r#"module native_logic;
        func side_effect() -> bool { println("unexpected"); return true; }
        func main() -> i32 {
            let a = false && side_effect();
            let b = true || side_effect();
            if !a && b { return 0; }
            return 1;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_bool_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("bool.obj");
    let executable = root.join("bool.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "right side was evaluated: {:?}",
        output.stdout
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_never_function_uses_unreachable_cfg_without_a_runtime_value() {
    let source = "module native_never; func spin() -> never { while true {} } func choose(flag: bool) -> i32 { if flag { spin(); } else { return 0; } } func main() -> i32 { return choose(false); }";
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_never_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("never.obj");
    let executable = root.join("never.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_object_links_and_runs_with_calls_branches_loops_and_arithmetic() {
    let source = r#"
        module native_test;
        fn step(a: i32) -> i32 { return (a + 3 - 1) * 2; }
        fn main() -> i32 {
            let mut i: i32 = 0;
            let mut total: i32 = 0;
            while i < 3 {
                total = total + step(i);
                i = i + 1;
            }
            if total == 18 {
                println(total);
                println("native ok");
                return 0;
            } else {
                return 1;
            }
        }
    "#;
    let module =
        frontend::lower_to_ir_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_test_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("native_test.obj");
    let executable = root.join("native_test.exe");

    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file(), "COFF object was not emitted");
    assert!(executable.is_file(), "MSVC executable was not emitted");

    let output = Command::new(&executable).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {:?}; stderr: {:?}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "18\nnative ok\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn canonical_08_source_builds_and_runs_as_native_executable() {
    let source = r#"
        module canonical_native;
        const BASE: i32 = 40;
        func add(a: i32, b: i32) -> i32 { return a + b; }
        func main() -> i32 {
            let answer = add(BASE, 2);
            println(answer);
            println(17 % 5);
            println((3.9 as i32));
            let float_value = 7 as f64;
            println(float_value as i32);
            println((1 << 3) | (16 >> 2));
            println(-2147483648);
            return 0;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_canonical_native_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("canonical_native.obj");
    let executable = root.join("canonical_native.exe");

    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file(), "COFF object was not emitted");
    assert!(executable.is_file(), "MSVC executable was not emitted");
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {:?}; stderr: {:?}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "42\n2\n3\n7\n12\n-2147483648\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_unsigned_narrow_integer_object_links_and_runs() {
    let source = r#"
        module native_u8;
        func add(a: u8, b: u8) -> u8 { return a + b; }
        func main() -> i32 {
            let total: u8 = add(200, 55);
            let large: u64 = 18446744073709551615;
            let small: u64 = 18446744073709551614;
            let signed8: i8 = -128;
            let signed16: i16 = -32768;
            let unsigned16: u16 = 65535;
            let unsigned32: u32 = 4294967295;
            let signed_size: isize = -9223372036854775808;
            let unsigned_size: usize = 18446744073709551615;
            let sign_extended: i16 = signed8 as i16;
            let zero_extended: u64 = unsigned32 as u64;
            if total == 255 {
                if large > small {
                    if signed8 < -1 {
                        if sign_extended < -1 {
                            if signed16 < 0 {
                                if unsigned16 > 1 {
                                    if unsigned32 > unsigned16 as u32 {
                                        if zero_extended == (4294967295 as u64) {
                                            if signed_size < -1 {
                                                if unsigned_size == (large as usize) { return 0; } else { return 9; }
                                            } else { return 8; }
                                        } else { return 7; }
                                    } else { return 6; }
                                } else { return 5; }
                            } else { return 4; }
                        } else { return 3; }
                    } else { return 2; }
                } else { return 1; }
            } else { return 1; }
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_u8_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("native_u8.obj");
    let executable = root.join("native_u8.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file());
    assert!(executable.is_file());
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_i128_internal_literals_and_comparisons_work_without_i128_abi() {
    let source = r#"
        module native_i128_internal;
        func main() -> i32 {
            let max: u128 = 340282366920938463463374607431768211455;
            let below: u128 = 340282366920938463463374607431768211454;
            let small: i128 = 120;
            let sum: i128 = small + 7;
            let minimum: i128 = -170141183460469231731687303715884105728;
            let shifted: i128 = small << 3;
            let sign_shifted: i128 = -120 >> 3;
            let bitwise: i128 = (120 & 127) | 256;
            if max > below {
                if sum == 127 {
                    if minimum < -1 {
                        if shifted == 960 {
                            if sign_shifted == -15 {
                                if bitwise == 376 { return 0; } else { return 5; }
                            } else { return 4; }
                        } else { return 3; }
                    } else { return 6; }
                } else { return 2; }
            } else { return 1; }
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root =
        std::env::temp_dir().join(format!("jahe_native_i128_internal_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("native_i128_internal.obj");
    let executable = root.join("native_i128_internal.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file());
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_backend_reports_i128_function_abi_limit_precisely() {
    let source = r#"
        module native_i128_abi;
        func identity(value: i128) -> i128 { return value; }
        func main() -> i32 { return 0; }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let error = NativeBackend::default()
        .emit_object(&verified)
        .expect_err("default Windows x64 ABI must reject i128 function boundaries");
    assert!(error.to_string().contains("i128 ABI parameter or return"));
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_checked_i128_overflow_traps() {
    for (name, expression) in [
        ("signed", "170141183460469231731687303715884105727i"),
        ("unsigned", "340282366920938463463374607431768211455u"),
    ] {
        let (ty, literal) = match name {
            "signed" => ("i128", &expression[..expression.len() - 1]),
            _ => ("u128", &expression[..expression.len() - 1]),
        };
        let source = format!(
            "module native_{name}_i128_overflow; func main() -> i32 {{ let max: {ty} = {literal}; let over: {ty} = max + 1; return 0; }}"
        );
        let module = frontend::lower_to_ir(&source).unwrap();
        let verified = ir::verify::Verifier.verify(&module).unwrap();
        let root = std::env::temp_dir().join(format!(
            "jahe_native_{name}_i128_overflow_{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let object = root.join(format!("{name}.obj"));
        let executable = root.join(format!("{name}.exe"));
        NativeBackend::default()
            .build_executable(&verified, &object, &executable)
            .unwrap();
        let output = Command::new(&executable).output().unwrap();
        assert_ne!(
            output.status.code(),
            Some(0),
            "{name} i128 overflow returned normally"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_i128_division_and_remainder_execute_correctly() {
    let source = r#"
        module native_i128_division;
        func main() -> i32 {
            let max: u128 = 340282366920938463463374607431768211455;
            let quotient: u128 = max / 3;
            let remainder: u128 = max % 3;
            let signed_quotient: i128 = -120 / 7;
            let signed_remainder: i128 = -120 % 7;
            if quotient == 113427455640312821154458202477256070485 {
                if remainder == 0 {
                    if signed_quotient == -17 {
                        if signed_remainder == -1 { return 0; } else { return 4; }
                    } else { return 3; }
                } else { return 2; }
            } else { return 1; }
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_i128_div_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("i128_div.obj");
    let executable = root.join("i128_div.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    assert!(object.is_file());
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "stderr: {:?}", output.stderr);
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_i128_division_overflow_and_zero_divisor_trap() {
    let cases = [
        (
            "overflow",
            r#"module native_i128_div_overflow;
               func main() -> i32 {
                   let minimum: i128 = -170141183460469231731687303715884105728;
                   let minus_one: i128 = -1;
                   let value: i128 = minimum / minus_one;
                   return value as i32;
               }"#,
        ),
        (
            "zero",
            r#"module native_u128_div_zero;
               func main() -> i32 {
                   let value: u128 = 7;
                   let zero: u128 = 0;
                   let quotient: u128 = value / zero;
                   return quotient as i32;
               }"#,
        ),
    ];
    for (name, source) in cases {
        let module = frontend::lower_to_ir(source).unwrap();
        let verified = ir::verify::Verifier.verify(&module).unwrap();
        let root = std::env::temp_dir().join(format!(
            "jahe_native_i128_div_{name}_{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let object = root.join("division.obj");
        let executable = root.join("division.exe");
        NativeBackend::default()
            .build_executable(&verified, &object, &executable)
            .unwrap();
        let output = Command::new(&executable).output().unwrap();
        assert_ne!(output.status.code(), Some(0), "i128 {name} did not trap");
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_signed_to_unsigned_integer_cast_traps_on_negative_value() {
    let source = r#"
        module native_negative_cast;
        func main() -> i32 {
            let negative: i16 = -1;
            let invalid: u8 = negative as u8;
            return invalid as i32;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root =
        std::env::temp_dir().join(format!("jahe_native_negative_cast_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("negative_cast.obj");
    let executable = root.join("negative_cast.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(&executable).output().unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "negative-to-unsigned cast returned normally"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_out_of_range_cast_traps() {
    let source = r#"module native_cast_trap;
        func narrow(value: i64) -> i32 { return value as i32; }
        func main() -> i32 { return narrow(2147483648); }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_native_cast_trap_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("native_cast_trap.obj");
    let executable = root.join("native_cast_trap.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(executable).output().unwrap();
    assert!(
        !output.status.success(),
        "out-of-range native cast did not trap"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_invalid_shift_count_traps() {
    let root = std::env::temp_dir().join(format!("jahe_native_shift_trap_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for (name, expression) in [("bad_count", "1 << 32"), ("overflow", "1073741824 << 1")] {
        let source =
            format!("module native_shift_trap; func main() -> i32 {{ return {expression}; }}");
        let module = frontend::lower_to_ir(&source).unwrap();
        let verified = ir::verify::Verifier.verify(&module).unwrap();
        let object = root.join(format!("{name}.obj"));
        let executable = root.join(format!("{name}.exe"));
        NativeBackend::default()
            .build_executable(&verified, &object, &executable)
            .unwrap();
        let output = Command::new(executable).output().unwrap();
        assert!(
            !output.status.success(),
            "native shift case `{name}` did not trap"
        );
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_backend_rejects_unbound_external_functions_with_diagnostic() {
    let source = "module m; fn main() -> i32 { println(1); return 0; }";
    let mut module =
        frontend::lower_to_ir_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap();
    let call = module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|inst| matches!(inst.kind, InstructionKind::Call { .. }))
        .unwrap();
    if let InstructionKind::Call { function, .. } = &mut call.kind {
        *function = "host_echo".into();
    }
    module.externals.push(ExternalFunction::new(
        "host_echo",
        Signature::any_value(Type::Void),
        call.span,
    ));
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let error = NativeBackend::default().emit_object(&verified).unwrap_err();
    assert_eq!(error.kind, BackendErrorKind::NativeCodegen);
    assert!(error
        .diagnostics
        .iter()
        .any(|d| d.code == "NAT0002" && d.message.contains("host binding")));
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_runtime_exports_each_typed_print_entry() {
    use jahe_compiler::ir::{Constant, Type as IrType};

    let source = r#"module typed_print;
        fn main() -> i32 {
            println(1); println(1); println(1); println(1);
            println(true); println("runtime"); return 0;
        }
    "#;
    let mut module =
        frontend::lower_to_ir_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = &mut function.blocks[0];
    let values = block
        .instructions
        .iter()
        .filter_map(|inst| match &inst.kind {
            InstructionKind::Call { function, args } if function == "println" => {
                args.first().map(|v| v.id)
            }
            _ => None,
        })
        .take(4)
        .collect::<Vec<_>>();
    let desired = [
        (IrType::I32, Constant::Integer(5)),
        (IrType::I64, Constant::Integer(6)),
        (IrType::F32, Constant::Float(1.25)),
        (IrType::F64, Constant::Float(2.5)),
    ];
    for (value_id, (ty, constant)) in values.into_iter().zip(desired) {
        let definition = block
            .instructions
            .iter_mut()
            .find(|inst| {
                inst.result
                    .as_ref()
                    .is_some_and(|value| value.id == value_id)
            })
            .unwrap();
        definition.result.as_mut().unwrap().ty = ty;
        definition.kind = InstructionKind::Constant(constant);
        let call = block.instructions.iter_mut().find(|inst| matches!(&inst.kind,
            InstructionKind::Call { function, args } if function == "println" && args.iter().any(|v| v.id == value_id)
        )).unwrap();
        if let InstructionKind::Call { args, .. } = &mut call.kind {
            args.iter_mut()
                .find(|value| value.id == value_id)
                .unwrap()
                .ty = ty;
        }
    }
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let root = std::env::temp_dir().join(format!("jahe_runtime_io_test_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let object = root.join("typed_print.obj");
    let executable = root.join("typed_print.exe");
    NativeBackend::default()
        .build_executable(&verified, &object, &executable)
        .unwrap();
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "5\n6\n1.25\n2.5\ntrue\nruntime\n"
    );
}
