use jahe_compiler::backend::interpreter::{
    error::RuntimeErrorKind, value::RuntimeValue, Interpreter,
};
use jahe_compiler::{frontend, ir};
#[path = "common/mod.rs"]
mod common;

#[derive(Clone)]
struct IntegerType {
    name: &'static str,
    width: u32,
    signed: bool,
    min: String,
    max: String,
}

fn integer_types() -> Vec<IntegerType> {
    [
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
    ]
    .into_iter()
    .map(|(name, width, signed)| {
        let (min, max) = if signed {
            let min = if width == 128 {
                i128::MIN
            } else {
                -(1i128 << (width - 1))
            };
            let max = if width == 128 {
                i128::MAX
            } else {
                (1i128 << (width - 1)) - 1
            };
            (min.to_string(), max.to_string())
        } else {
            let max = if width == 128 {
                u128::MAX
            } else {
                (1u128 << width) - 1
            };
            ("0".to_string(), max.to_string())
        };
        IntegerType {
            name,
            width,
            signed,
            min,
            max,
        }
    })
    .collect()
}

#[derive(Clone)]
struct NativeCase {
    name: String,
    source: String,
    expected_runtime: Result<(), RuntimeErrorKind>,
}

fn valid_binary_bundle(ty: &IntegerType, name: &str, pairs: &[(&str, &str, &str)]) -> NativeCase {
    let mut source = format!("module boundary_{name};\n");
    let mut checks = String::new();
    for (index, (op, lhs, rhs)) in pairs.iter().enumerate() {
        let result_ty = if matches!(*op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
            "bool"
        } else {
            ty.name
        };
        source.push_str(&format!(
            "const LEFT{index}: {} = {lhs}; const RIGHT{index}: {} = {rhs}; const EXPECTED{index}: {result_ty} = LEFT{index} {op} RIGHT{index};\n",
            ty.name, ty.name
        ));
        checks.push_str(&format!(
            "let lhs{index}: {} = LEFT{index}; let rhs{index}: {} = RIGHT{index}; if ((lhs{index} {op} rhs{index}) != EXPECTED{index}) {{ return {}; }}\n",
            ty.name,
            ty.name,
            index + 1
        ));
    }
    source.push_str(&format!("func main() -> i32 {{ {checks} return 0; }}\n"));
    NativeCase {
        name: format!("{}_{}_valid_edges", ty.name, name),
        source,
        expected_runtime: Ok(()),
    }
}

fn invalid_integer_case(
    ty: &IntegerType,
    name: &str,
    lhs: &str,
    op: &str,
    rhs: &str,
    expected: RuntimeErrorKind,
) -> NativeCase {
    let source = format!(
        "module invalid_{name}; const LEFT: {} = {lhs}; const RIGHT: {} = {rhs}; func main() -> i32 {{ let lhs: {} = LEFT; let rhs: {} = RIGHT; let result: {} = lhs {op} rhs; return 0; }}",
        ty.name, ty.name, ty.name, ty.name, ty.name
    );
    NativeCase {
        name: format!("{}_{}", ty.name, name),
        source,
        expected_runtime: Err(expected),
    }
}

fn unary_negation_case(ty: &IntegerType, value: &str, should_trap: bool) -> NativeCase {
    let source = format!(
        "module unary_boundary; const VALUE: {} = {value}; func main() -> i32 {{ let value: {} = VALUE; let result: {} = -value; return 0; }}",
        ty.name, ty.name, ty.name
    );
    NativeCase {
        name: format!(
            "{}_neg_{}",
            ty.name,
            if should_trap { "trap" } else { "valid" }
        ),
        source,
        expected_runtime: if should_trap {
            Err(RuntimeErrorKind::IntegerOverflow)
        } else {
            Ok(())
        },
    }
}

fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::new();
    for ty in integer_types() {
        let zero = "0";
        let one = "1";
        let minus_one = "-1";
        let signed_valid = [
            ("+", ty.min.as_str(), zero),
            ("+", ty.max.as_str(), zero),
            ("-", ty.min.as_str(), zero),
            ("-", ty.max.as_str(), zero),
            ("*", ty.min.as_str(), one),
            ("*", ty.max.as_str(), one),
            ("/", ty.min.as_str(), one),
            ("/", ty.max.as_str(), one),
            ("%", ty.min.as_str(), "3"),
            ("%", ty.max.as_str(), "3"),
            ("%", ty.min.as_str(), "-1"),
        ];
        let unsigned_valid = [
            ("+", ty.max.as_str(), zero),
            ("-", ty.max.as_str(), zero),
            ("*", ty.max.as_str(), one),
            ("/", ty.max.as_str(), one),
            ("%", ty.max.as_str(), "3"),
        ];
        cases.push(valid_binary_bundle(
            &ty,
            "arithmetic",
            if ty.signed {
                &signed_valid
            } else {
                &unsigned_valid
            },
        ));

        let comparisons = [
            ("==", ty.min.as_str(), ty.min.as_str()),
            ("!=", ty.min.as_str(), ty.max.as_str()),
            ("<", ty.min.as_str(), ty.max.as_str()),
            ("<=", ty.min.as_str(), ty.max.as_str()),
            (">", ty.max.as_str(), ty.min.as_str()),
            (">=", ty.max.as_str(), ty.min.as_str()),
            ("&", ty.min.as_str(), ty.max.as_str()),
            ("|", ty.min.as_str(), ty.max.as_str()),
            ("^", ty.min.as_str(), ty.max.as_str()),
        ];
        cases.push(valid_binary_bundle(&ty, "comparison_bitwise", &comparisons));

        let count = (ty.width - 1).to_string();
        let shifts = if ty.signed {
            vec![
                ("<<", minus_one, count.as_str()),
                (">>", ty.min.as_str(), one),
            ]
        } else {
            vec![("<<", one, count.as_str()), (">>", ty.max.as_str(), one)]
        };
        cases.push(valid_binary_bundle(&ty, "shift_edges", &shifts));

        cases.push(unary_negation_case(
            &ty,
            if ty.signed { &ty.max } else { zero },
            false,
        ));

        if ty.signed {
            cases.push(invalid_integer_case(
                &ty,
                "add_overflow",
                &ty.max,
                "+",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "sub_underflow",
                &ty.min,
                "-",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "mul_overflow",
                &ty.max,
                "*",
                "2",
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "mul_underflow",
                &ty.min,
                "*",
                "2",
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "div_zero",
                &ty.min,
                "/",
                zero,
                RuntimeErrorKind::DivisionByZero,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "rem_zero",
                &ty.min,
                "%",
                zero,
                RuntimeErrorKind::DivisionByZero,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "min_div_neg_one",
                &ty.min,
                "/",
                minus_one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "left_shift_overflow",
                &ty.max,
                "<<",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            for (name, op, count) in [
                ("left_shift_count_width", "<<", ty.width.to_string()),
                ("right_shift_count_width", ">>", ty.width.to_string()),
                ("negative_shift_count", "<<", minus_one.to_string()),
            ] {
                cases.push(invalid_integer_case(
                    &ty,
                    name,
                    one,
                    op,
                    &count,
                    RuntimeErrorKind::InvalidShift,
                ));
            }
            cases.push(unary_negation_case(&ty, &ty.min, true));
        } else {
            cases.push(invalid_integer_case(
                &ty,
                "add_overflow",
                &ty.max,
                "+",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "sub_underflow",
                zero,
                "-",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "mul_overflow",
                &ty.max,
                "*",
                "2",
                RuntimeErrorKind::IntegerOverflow,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "div_zero",
                &ty.max,
                "/",
                zero,
                RuntimeErrorKind::DivisionByZero,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "rem_zero",
                &ty.max,
                "%",
                zero,
                RuntimeErrorKind::DivisionByZero,
            ));
            cases.push(invalid_integer_case(
                &ty,
                "left_shift_overflow",
                &ty.max,
                "<<",
                one,
                RuntimeErrorKind::IntegerOverflow,
            ));
            for op in ["<<", ">>"] {
                cases.push(invalid_integer_case(
                    &ty,
                    if op == "<<" {
                        "left_shift_count_width"
                    } else {
                        "right_shift_count_width"
                    },
                    one,
                    op,
                    &ty.width.to_string(),
                    RuntimeErrorKind::InvalidShift,
                ));
            }
            cases.push(unary_negation_case(&ty, one, true));
        }
    }
    cases
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_integer_operator_boundaries_match_constant_interpreter_and_native() {
    run_native_cases(native_cases());
}

fn unsigned_max(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}

fn signed_min(width: u32) -> i128 {
    if width == 128 {
        i128::MIN
    } else {
        -(1i128 << (width - 1))
    }
}

fn signed_max(width: u32) -> i128 {
    if width == 128 {
        i128::MAX
    } else {
        (1i128 << (width - 1)) - 1
    }
}

fn valid_integer_cast_values(source: &IntegerType, target: &IntegerType) -> Vec<String> {
    let mut values = Vec::new();
    if source.signed && target.signed {
        let lower = signed_min(source.width).max(signed_min(target.width));
        let upper = signed_max(source.width).min(signed_max(target.width));
        values.push(lower.to_string());
        if upper != lower {
            values.push(upper.to_string());
        }
    } else {
        values.push("0".into());
        let source_upper = if source.signed {
            signed_max(source.width) as u128
        } else {
            unsigned_max(source.width)
        };
        let target_upper = if target.signed {
            signed_max(target.width) as u128
        } else {
            unsigned_max(target.width)
        };
        let upper = source_upper.min(target_upper);
        if upper != 0 {
            values.push(upper.to_string());
        }
    }
    values.sort();
    values.dedup();
    values
}

fn invalid_integer_cast_values(source: &IntegerType, target: &IntegerType) -> Vec<String> {
    let mut values = Vec::new();
    if source.signed && target.signed {
        let source_min = signed_min(source.width);
        let source_max = signed_max(source.width);
        let target_min = signed_min(target.width);
        let target_max = signed_max(target.width);
        if source_min < target_min {
            values.push((target_min - 1).to_string());
        }
        if source_max > target_max {
            values.push((target_max + 1).to_string());
        }
    } else if source.signed {
        if signed_min(source.width) < 0 {
            values.push(signed_min(source.width).to_string());
        }
        let source_max = signed_max(source.width) as u128;
        let target_max = unsigned_max(target.width);
        if source_max > target_max {
            values.push((target_max + 1).to_string());
        }
    } else {
        let source_max = unsigned_max(source.width);
        let target_max = if target.signed {
            signed_max(target.width) as u128
        } else {
            unsigned_max(target.width)
        };
        if source_max > target_max {
            values.push((target_max + 1).to_string());
        }
    }
    values
}

fn integer_cast_boundary_cases() -> Vec<NativeCase> {
    let types = integer_types();
    let mut cases = Vec::new();
    for source_type in &types {
        for target_type in &types {
            let values = valid_integer_cast_values(source_type, target_type);
            let mut source = format!("module cast_edges;\n");
            let mut checks = String::new();
            for (index, value) in values.iter().enumerate() {
                source.push_str(&format!(
                    "const SOURCE{index}: {} = {value}; const EXPECTED{index}: {} = SOURCE{index} as {};\n",
                    source_type.name, target_type.name, target_type.name
                ));
                checks.push_str(&format!(
                    "let input{index}: {} = SOURCE{index}; let converted{index}: {} = input{index} as {}; if converted{index} != EXPECTED{index} {{ return {}; }}\n",
                    source_type.name,
                    target_type.name,
                    target_type.name,
                    index + 1
                ));
            }
            source.push_str(&format!("func main() -> i32 {{ {checks} return 0; }}"));
            cases.push(NativeCase {
                name: format!(
                    "cast_{}_to_{}_valid_edges",
                    source_type.name, target_type.name
                ),
                source,
                expected_runtime: Ok(()),
            });
            for (index, value) in invalid_integer_cast_values(source_type, target_type)
                .into_iter()
                .enumerate()
            {
                let source = format!(
                    "module cast_out_of_range; const SOURCE: {} = {value}; func main() -> i32 {{ let input: {} = SOURCE; let invalid: {} = input as {}; return 0; }}",
                    source_type.name,
                    source_type.name,
                    target_type.name,
                    target_type.name
                );
                cases.push(NativeCase {
                    name: format!(
                        "cast_{}_to_{}_out_of_range_{index}",
                        source_type.name, target_type.name
                    ),
                    source,
                    expected_runtime: Err(RuntimeErrorKind::InvalidCast),
                });
            }
        }
    }
    cases
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_integer_cast_boundaries_match_constant_interpreter_and_native() {
    run_native_cases(integer_cast_boundary_cases());
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_signed_minimum_remainder_by_negative_one_is_zero() {
    for ty in integer_types().into_iter().filter(|ty| ty.signed) {
        let source = format!(
            "module min_remainder; const MIN: {0} = {1}; const EXPECTED: {0} = MIN % -1; func main() -> i32 {{ let min: {0} = MIN; let rem: {0} = min % -1; if rem == EXPECTED {{ return 0; }} return 1; }}",
            ty.name, ty.min
        );
        let module = frontend::lower_to_ir(&source).unwrap();
        let interpreted = Interpreter::default().execute(&module).unwrap();
        assert_eq!(
            interpreted.return_value,
            Some(RuntimeValue::I32(0)),
            "{}",
            ty.name
        );
        let output =
            common::run_native_case(&format!("{}_min_rem_neg_one", ty.name), &source).unwrap();
        assert_eq!(output.status.code(), Some(0), "{}: {output:?}", ty.name);
    }
}

fn run_native_cases(cases: Vec<NativeCase>) {
    let mut workers = (0..8).map(|_| Vec::new()).collect::<Vec<Vec<_>>>();
    let worker_count = workers.len();
    for (index, case) in cases.into_iter().enumerate() {
        workers[index % worker_count].push((index, case));
    }
    let failures = std::thread::scope(|scope| {
        workers
            .into_iter()
            .map(|worker| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for (index, case) in worker {
                        let id = format!("{}_{index}", case.name);
                        let module = match frontend::lower_to_ir(&case.source) {
                            Ok(module) => module,
                            Err(error) => {
                                failures.push(format!("{id}: frontend: {error:?}"));
                                continue;
                            }
                        };
                        let _verified = match ir::verify::Verifier.verify(&module) {
                            Ok(verified) => verified,
                            Err(error) => {
                                failures.push(format!("{id}: verifier: {error:?}"));
                                continue;
                            }
                        };
                        let interpreted = Interpreter::default().execute(&module);
                        match (&case.expected_runtime, interpreted) {
                            (Ok(()), Ok(result))
                                if result.return_value == Some(RuntimeValue::I32(0)) => {}
                            (Err(expected), Err(error))
                                if error.runtime.as_ref().map(|runtime| &runtime.kind)
                                    == Some(expected) => {}
                            (expected, actual) => {
                                failures.push(format!(
                                    "{id}: interpreter expected {expected:?}, got {actual:?}"
                                ));
                                continue;
                            }
                        }
                        match common::run_native_case(&id, &case.source) {
                            Ok(output) if output.status.success() == case.expected_runtime.is_ok() => {}
                            Ok(output) => failures.push(format!(
                                "{id}: native expected_success={}, exit={:?}, stdout={:?}, stderr={:?}",
                                case.expected_runtime.is_ok(),
                                output.status.code(),
                                String::from_utf8_lossy(&output.stdout),
                                String::from_utf8_lossy(&output.stderr)
                            )),
                            Err(error) => failures.push(format!("{id}: {error}")),
                        }
                    }
                    failures
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().expect("boundary worker panicked"))
            .collect::<Vec<_>>()
    });
    assert!(
        failures.is_empty(),
        "boundary parity failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
