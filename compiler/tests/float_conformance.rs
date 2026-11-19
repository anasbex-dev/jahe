use jahe_compiler::{
    backend::interpreter::{error::RuntimeErrorKind, value::RuntimeValue, Interpreter},
    frontend,
};
#[path = "common/mod.rs"]
mod common;

#[derive(Clone, Copy)]
struct FloatType {
    name: &'static str,
    bits: u32,
}

#[derive(Clone, Copy)]
struct IntType {
    name: &'static str,
    bits: u32,
    signed: bool,
}

const FLOATS: [FloatType; 2] = [
    FloatType {
        name: "f32",
        bits: 32,
    },
    FloatType {
        name: "f64",
        bits: 64,
    },
];
const INTEGERS: [IntType; 10] = [
    IntType {
        name: "i8",
        bits: 8,
        signed: true,
    },
    IntType {
        name: "i16",
        bits: 16,
        signed: true,
    },
    IntType {
        name: "i32",
        bits: 32,
        signed: true,
    },
    IntType {
        name: "i64",
        bits: 64,
        signed: true,
    },
    IntType {
        name: "isize",
        bits: 64,
        signed: true,
    },
    IntType {
        name: "u8",
        bits: 8,
        signed: false,
    },
    IntType {
        name: "u16",
        bits: 16,
        signed: false,
    },
    IntType {
        name: "u32",
        bits: 32,
        signed: false,
    },
    IntType {
        name: "u64",
        bits: 64,
        signed: false,
    },
    IntType {
        name: "usize",
        bits: 64,
        signed: false,
    },
];

#[derive(Clone)]
struct CastCase {
    name: String,
    source: String,
    target: &'static str,
    valid: bool,
    expected: String,
}

fn next_up_f64(value: f64) -> f64 {
    if value.is_nan() || value == f64::INFINITY {
        return value;
    }
    if value == 0.0 {
        return f64::from_bits(1);
    }
    f64::from_bits(if value > 0.0 {
        value.to_bits() + 1
    } else {
        value.to_bits() - 1
    })
}

fn next_down_f64(value: f64) -> f64 {
    if value.is_nan() || value == f64::NEG_INFINITY {
        return value;
    }
    if value == 0.0 {
        return -f64::from_bits(1);
    }
    f64::from_bits(if value > 0.0 {
        value.to_bits() - 1
    } else {
        value.to_bits() + 1
    })
}

fn next_up_f32(value: f32) -> f32 {
    if value.is_nan() || value == f32::INFINITY {
        return value;
    }
    if value == 0.0 {
        return f32::from_bits(1);
    }
    f32::from_bits(if value > 0.0 {
        value.to_bits() + 1
    } else {
        value.to_bits() - 1
    })
}

fn next_down_f32(value: f32) -> f32 {
    if value.is_nan() || value == f32::NEG_INFINITY {
        return value;
    }
    if value == 0.0 {
        return -f32::from_bits(1);
    }
    f32::from_bits(if value > 0.0 {
        value.to_bits() - 1
    } else {
        value.to_bits() + 1
    })
}

fn literal(value: f64, ty: FloatType) -> String {
    // The lexer intentionally accepts decimal-point literals without exponent notation.
    // Enough fractional digits preserve even f64 subnormals and adjacent boundary values.
    let precision = if ty.bits == 32 { 70 } else { 1100 };
    format!("{value:.precision$}")
}

fn source_value(value: f64, ty: FloatType) -> String {
    if value.is_nan() {
        "0.0 / 0.0".into()
    } else if value == f64::INFINITY {
        "1.0 / 0.0".into()
    } else if value == f64::NEG_INFINITY {
        "-1.0 / 0.0".into()
    } else {
        literal(value, ty)
    }
}

fn bounds(target: IntType) -> (f64, f64) {
    if target.signed {
        let upper = 2f64.powi(target.bits as i32 - 1);
        (-upper, upper)
    } else {
        (0.0, 2f64.powi(target.bits as i32))
    }
}

fn cast_cases() -> Vec<CastCase> {
    let mut cases = Vec::new();
    for float in FLOATS {
        for target in INTEGERS {
            let (lower, upper) = bounds(target);
            let candidates = if float.bits == 32 {
                let lo = lower as f32;
                let hi = upper as f32;
                vec![
                    next_down_f32(lo) as f64,
                    lo as f64,
                    next_up_f32(lo) as f64,
                    next_down_f32(hi) as f64,
                    hi as f64,
                    next_up_f32(hi) as f64,
                    -0.0,
                    0.0,
                    -0.5,
                    0.5,
                    f32::from_bits(1) as f64,
                    -f32::from_bits(1) as f64,
                    f32::INFINITY as f64,
                    f32::NEG_INFINITY as f64,
                    f32::NAN as f64,
                ]
            } else {
                vec![
                    next_down_f64(lower),
                    lower,
                    next_up_f64(lower),
                    next_down_f64(upper),
                    upper,
                    next_up_f64(upper),
                    -0.0,
                    0.0,
                    -0.5,
                    0.5,
                    f64::from_bits(1),
                    -f64::from_bits(1),
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                ]
            };
            let mut seen = std::collections::HashSet::new();
            for (index, value) in candidates.into_iter().enumerate() {
                let key = if float.bits == 32 {
                    (value as f32).to_bits() as u64
                } else {
                    value.to_bits()
                };
                if !seen.insert(key) {
                    continue;
                }
                let valid = value.is_finite() && value >= lower && value < upper;
                let result = if target.signed {
                    value.trunc() as i128 as i64
                } else {
                    value.trunc() as u64 as i64
                };
                let expected = if valid {
                    result.to_string()
                } else {
                    "0".into()
                };
                let value_source = source_value(value, float);
                let program = if valid {
                    format!("module float_cast; const SOURCE: {f} = {v}; const EXPECTED: {i} = SOURCE as {i}; func main() -> i32 {{ let source: {f} = SOURCE; let result: {i} = source as {i}; if result == EXPECTED {{ return 0; }} return 1; }}", f=float.name, i=target.name, v=value_source)
                } else {
                    format!("module float_cast; const SOURCE: {f} = {v}; func main() -> i32 {{ let source: {f} = SOURCE; let result: {i} = source as {i}; return 0; }}", f=float.name, i=target.name, v=value_source)
                };
                cases.push(CastCase {
                    name: format!("{}_{}_edge_{index}_bits_{key:x}", float.name, target.name),
                    source: program,
                    target: target.name,
                    valid,
                    expected,
                });
            }
        }
    }
    cases
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn float_to_integer_edges_match_constant_interpreter_and_native() {
    let mut workers = (0..8).map(|_| Vec::new()).collect::<Vec<Vec<_>>>();
    for (index, case) in cast_cases().into_iter().enumerate() {
        workers[index % 8].push(case);
    }
    let failures = std::thread::scope(|scope| {
        workers
            .into_iter()
            .map(|worker| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for case in worker {
                        let module = match frontend::lower_to_ir(&case.source) {
                            Ok(module) => module,
                            Err(error) => {
                                failures.push(format!("{}: compile: {error:?}", case.name));
                                continue;
                            }
                        };
                        let interpreted = Interpreter::default().execute(&module);
                        if case.valid {
                            match interpreted {
                                Ok(result) if result.return_value == Some(RuntimeValue::I32(0)) => {
                                }
                                other => failures.push(format!(
                                    "{}: interpreter expected {}, got {other:?}",
                                    case.name, case.expected
                                )),
                            }
                        } else {
                            match interpreted {
                                Err(error)
                                    if error.runtime.as_ref().is_some_and(|e| {
                                        e.kind == RuntimeErrorKind::InvalidCast
                                    }) => {}
                                other => failures.push(format!(
                                    "{}: expected InvalidCast, got {other:?}",
                                    case.name
                                )),
                            }
                            let bad = case.source.replace(
                                "func main()",
                                &format!(
                                    "const BAD: {0} = SOURCE as {0}; func main()",
                                    case.target
                                ),
                            );
                            let diagnostics = frontend::analyze(&bad).unwrap_or_default();
                            if !diagnostics.iter().any(|d| d.code == "E0708") {
                                failures.push(format!(
                                    "{}: constant evaluator did not report E0708: {diagnostics:?}",
                                    case.name
                                ));
                            }
                        }
                        match common::run_native_case(&case.name, &case.source) {
                            Ok(output) if output.status.success() == case.valid => {}
                            Ok(output) => failures.push(format!(
                                "{}: native valid={}, status={:?}, stdout={:?}, stderr={:?}",
                                case.name,
                                case.valid,
                                output.status.code(),
                                String::from_utf8_lossy(&output.stdout),
                                String::from_utf8_lossy(&output.stderr)
                            )),
                            Err(error) => failures.push(format!("{}: {error}", case.name)),
                        }
                    }
                    failures
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(
        failures.is_empty(),
        "float-to-integer mismatches ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn integer_to_float_precision_edges_match_constants_and_interpreter() {
    let mut cases = Vec::new();
    for source in [
        IntType {
            name: "i8",
            bits: 8,
            signed: true,
        },
        IntType {
            name: "i16",
            bits: 16,
            signed: true,
        },
        IntType {
            name: "i32",
            bits: 32,
            signed: true,
        },
        IntType {
            name: "i64",
            bits: 64,
            signed: true,
        },
        IntType {
            name: "i128",
            bits: 128,
            signed: true,
        },
        IntType {
            name: "isize",
            bits: 64,
            signed: true,
        },
        IntType {
            name: "u8",
            bits: 8,
            signed: false,
        },
        IntType {
            name: "u16",
            bits: 16,
            signed: false,
        },
        IntType {
            name: "u32",
            bits: 32,
            signed: false,
        },
        IntType {
            name: "u64",
            bits: 64,
            signed: false,
        },
        IntType {
            name: "u128",
            bits: 128,
            signed: false,
        },
        IntType {
            name: "usize",
            bits: 64,
            signed: false,
        },
    ] {
        let (min, max) = if source.signed {
            if source.bits == 128 {
                (i128::MIN.to_string(), i128::MAX.to_string())
            } else {
                (
                    (-(1i128 << (source.bits - 1))).to_string(),
                    ((1i128 << (source.bits - 1)) - 1).to_string(),
                )
            }
        } else if source.bits == 128 {
            ("0".to_owned(), u128::MAX.to_string())
        } else {
            ("0".to_owned(), ((1u128 << source.bits) - 1).to_string())
        };
        let min_value = min.parse::<i128>().unwrap_or(i128::MIN);
        let max_value = max.parse::<i128>().unwrap_or(i128::MAX);
        let max_digits = max.len();
        let mut values = vec![min, max, "0".into(), "1".into()];
        if source.signed {
            values.push("-1".into());
        }
        for p in [24u32, 53] {
            let base = 1i128 << p;
            for value in [base - 1, base, base + 1, -base - 1, -base, -base + 1] {
                if source.signed && value >= min_value && value <= max_value {
                    values.push(value.to_string());
                } else if !source.signed && value >= 0 && value.to_string().len() <= max_digits {
                    values.push(value.to_string());
                }
            }
        }
        values.sort();
        values.dedup();
        for float in FLOATS {
            for value in &values {
                let source_literal = value.clone();
                let program = format!("module integer_float; const INPUT: {i} = {v}; const EXPECTED: {f} = INPUT as {f}; func main() -> i32 {{ let input: {i} = INPUT; let actual: {f} = input as {f}; if actual == EXPECTED {{ return 0; }} return 1; }}", i=source.name, v=source_literal, f=float.name);
                cases.push((source, float, value.clone(), program));
            }
        }
    }
    let mut workers = (0..8).map(|_| Vec::new()).collect::<Vec<Vec<_>>>();
    for (index, case) in cases.into_iter().enumerate() {
        workers[index % 8].push(case);
    }
    let failures = std::thread::scope(|scope| {
        workers
            .into_iter()
            .map(|worker| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for (source, float, value, program) in worker {
                        let name = format!("{} -> {} for {value}", source.name, float.name);
                        let module = match frontend::lower_to_ir(&program) {
                            Ok(module) => module,
                            Err(error) => {
                                failures.push(format!("{name}: {error:?}"));
                                continue;
                            }
                        };
                        match Interpreter::default().execute(&module) {
                            Ok(result) if result.return_value == Some(RuntimeValue::I32(0)) => {}
                            other => failures.push(format!("{name}: interpreter {other:?}")),
                        }
                        if source.bits < 128 {
                            match common::run_native_case(
                                &format!("int_to_{}_{}_{}", source.name, float.name, value),
                                &program,
                            ) {
                                Ok(output) if output.status.success() => {}
                                Ok(output) => failures.push(format!("{name}: native {output:?}")),
                                Err(error) => failures.push(format!("{name}: {error}")),
                            }
                        }
                    }
                    failures
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(
        failures.is_empty(),
        "integer-to-float mismatches ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn float_arithmetic_ieee_edges_match_constant_interpreter_and_native() {
    for float in FLOATS {
        let (smallest_subnormal, largest_subnormal, smallest_normal, largest_finite) =
            if float.bits == 32 {
                let f = f32::from_bits(1);
                (
                    f as f64,
                    f32::from_bits(0x007f_ffff) as f64,
                    f32::MIN_POSITIVE as f64,
                    f32::MAX as f64,
                )
            } else {
                (
                    f64::from_bits(1),
                    f64::from_bits(0x000f_ffff_ffff_ffff),
                    f64::MIN_POSITIVE,
                    f64::MAX,
                )
            };
        let cases = [
            ("positive_zero_add", "0.0", "0.0", "+"),
            ("negative_zero_add", "-0.0", "-0.0", "+"),
            ("mixed_zero_add", "0.0", "-0.0", "+"),
            ("negative_zero_subtract", "-0.0", "0.0", "-"),
            ("positive_zero_subtract", "0.0", "-0.0", "-"),
            ("negative_zero_multiply", "-0.0", "1.0", "*"),
            ("negative_zero_sign_flip_multiply", "-0.0", "-1.0", "*"),
            ("positive_zero_divide", "0.0", "1.0", "/"),
            ("negative_zero_divide", "-0.0", "1.0", "/"),
            ("negative_zero_sign_flip_divide", "-0.0", "-1.0", "/"),
            (
                "subnormal_add",
                &literal(smallest_subnormal, float),
                &literal(smallest_subnormal, float),
                "+",
            ),
            (
                "subnormal_cancel",
                &literal(smallest_subnormal, float),
                &literal(smallest_subnormal, float),
                "-",
            ),
            (
                "positive_underflow_multiply",
                &literal(smallest_subnormal, float),
                "0.5",
                "*",
            ),
            (
                "negative_underflow_multiply",
                &literal(-smallest_subnormal, float),
                "0.5",
                "*",
            ),
            (
                "underflow_divide",
                &literal(smallest_subnormal, float),
                "2.0",
                "/",
            ),
            (
                "normal_to_subnormal",
                &literal(smallest_normal, float),
                "2.0",
                "/",
            ),
            (
                "subnormal_round_to_normal",
                &literal(largest_subnormal, float),
                &literal(smallest_subnormal, float),
                "+",
            ),
            (
                "finite_overflow_multiply",
                &literal(largest_finite, float),
                "2.0",
                "*",
            ),
            (
                "finite_overflow_add",
                &literal(largest_finite, float),
                &literal(largest_finite, float),
                "+",
            ),
            (
                "negative_finite_overflow",
                &literal(-largest_finite, float),
                "2.0",
                "*",
            ),
            (
                "finite_subtract_to_overflow",
                &literal(largest_finite, float),
                &literal(-largest_finite, float),
                "-",
            ),
            ("positive_infinity", "1.0", "0.0", "/"),
            ("negative_infinity", "-1.0", "0.0", "/"),
            ("negative_zero_divisor", "1.0", "-0.0", "/"),
            ("negative_zero_divisor_sign_flip", "-1.0", "-0.0", "/"),
            ("infinity_times_zero", "1.0 / 0.0", "0.0", "*"),
            ("nan_propagation", "0.0 / 0.0", "1.0", "+"),
        ];
        for (name, left, right, op) in cases {
            let source = format!(
                "module float_edges; const LEFT: {f} = {left}; const RIGHT: {f} = {right}; const EXPECTED: {f} = LEFT {op} RIGHT; func main() -> i32 {{ let left: {f} = LEFT; let right: {f} = RIGHT; let actual: {f} = left {op} right; println(EXPECTED); println(actual); return 0; }}",
                f = float.name
            );
            let module = frontend::lower_to_ir(&source)
                .unwrap_or_else(|error| panic!("{} {name}: {error:?}", float.name));
            let interpreted = Interpreter::default()
                .execute(&module)
                .unwrap_or_else(|error| panic!("{} {name}: {error:?}", float.name));
            let lines = interpreted.output.lines().collect::<Vec<_>>();
            assert_eq!(
                lines.len(),
                2,
                "{} {name}: {:?}",
                float.name,
                interpreted.output
            );
            assert_eq!(
                lines[0], lines[1],
                "{} {name}: {:?}",
                float.name, interpreted.output
            );
            let native =
                common::run_native_case(&format!("{}_arithmetic_{name}", float.name), &source)
                    .unwrap_or_else(|error| panic!("{} {name}: {error}", float.name));
            assert!(native.status.success(), "{} {name}: {native:?}", float.name);
            assert_eq!(
                String::from_utf8_lossy(&native.stdout)
                    .lines()
                    .collect::<Vec<_>>(),
                lines,
                "{} {name}",
                float.name
            );
        }
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn float_comparisons_with_nan_match_constant_interpreter_and_native() {
    for float in FLOATS {
        for (op, expected, nan_left) in [
            ("==", "false", false),
            ("!=", "true", false),
            ("<", "false", false),
            ("<=", "false", false),
            (">", "false", false),
            (">=", "false", false),
            ("==", "false", true),
            ("!=", "true", true),
            ("<", "false", true),
            ("<=", "false", true),
            (">", "false", true),
            (">=", "false", true),
        ] {
            let (constant_left, constant_right, left, right) = if nan_left {
                ("NAN_VALUE", "ONE", "nan_value", "one")
            } else {
                ("ONE", "NAN_VALUE", "one", "nan_value")
            };
            let source = format!(
                "module nan_compare; const NAN_VALUE: {f} = 0.0 / 0.0; const ONE: {f} = 1.0; const EXPECTED: bool = {constant_left} {op} {constant_right}; func main() -> i32 {{ let nan_value: {f} = NAN_VALUE; let one: {f} = ONE; if (({left} {op} {right}) == EXPECTED) && (EXPECTED == {expected}) {{ return 0; }} return 1; }}",
                f = float.name
            );
            let module = frontend::lower_to_ir(&source).unwrap();
            let interpreted = Interpreter::default().execute(&module).unwrap();
            assert_eq!(
                interpreted.return_value,
                Some(RuntimeValue::I32(0)),
                "{} {op}",
                float.name
            );
            let native =
                common::run_native_case(&format!("{}_nan_compare_{}", float.name, op), &source)
                    .unwrap();
            assert!(native.status.success(), "{} {op}: {native:?}", float.name);
        }
    }
}

#[test]
fn float_to_i128_u128_edges_match_constant_evaluator_and_interpreter() {
    for float in FLOATS {
        for target in [
            IntType {
                name: "i128",
                bits: 128,
                signed: true,
            },
            IntType {
                name: "u128",
                bits: 128,
                signed: false,
            },
        ] {
            let (lower, upper) = bounds(target);
            let candidates = if float.bits == 32 {
                let lo = lower as f32;
                let hi = upper as f32;
                vec![
                    next_down_f32(lo) as f64,
                    lo as f64,
                    next_up_f32(lo) as f64,
                    next_down_f32(hi) as f64,
                    hi as f64,
                    next_up_f32(hi) as f64,
                    -0.0,
                    0.0,
                    -0.5,
                    0.5,
                    f32::INFINITY as f64,
                    f32::NEG_INFINITY as f64,
                    f32::NAN as f64,
                ]
            } else {
                vec![
                    next_down_f64(lower),
                    lower,
                    next_up_f64(lower),
                    next_down_f64(upper),
                    upper,
                    next_up_f64(upper),
                    -0.0,
                    0.0,
                    -0.5,
                    0.5,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                ]
            };
            let mut seen = std::collections::HashSet::new();
            for value in candidates {
                let key = if float.bits == 32 {
                    (value as f32).to_bits() as u64
                } else {
                    value.to_bits()
                };
                if !seen.insert(key) {
                    continue;
                }
                let valid = value.is_finite() && value >= lower && value < upper;
                let source = if valid {
                    format!("module wide_float_cast; const SOURCE: {f} = {v}; const EXPECTED: {i} = SOURCE as {i}; func main() -> i32 {{ let source: {f} = SOURCE; let actual: {i} = source as {i}; if actual == EXPECTED {{ return 0; }} return 1; }}", f=float.name, i=target.name, v=source_value(value, float))
                } else {
                    format!("module wide_float_cast; const SOURCE: {f} = {v}; func main() -> i32 {{ let source: {f} = SOURCE; let actual: {i} = source as {i}; return 0; }}", f=float.name, i=target.name, v=source_value(value, float))
                };
                let module = frontend::lower_to_ir(&source).unwrap_or_else(|error| {
                    panic!("{} -> {} {key:x}: {error:?}", float.name, target.name)
                });
                match (valid, Interpreter::default().execute(&module)) {
                    (true, Ok(result)) if result.return_value == Some(RuntimeValue::I32(0)) => {}
                    (false, Err(error))
                        if error
                            .runtime
                            .as_ref()
                            .is_some_and(|e| e.kind == RuntimeErrorKind::InvalidCast) => {}
                    (expected, actual) => panic!(
                        "{} -> {} valid={expected} bits={key:x}: {actual:?}",
                        float.name, target.name
                    ),
                }
                if !valid {
                    let bad = source.replace(
                        "func main()",
                        &format!("const BAD: {0} = SOURCE as {0}; func main()", target.name),
                    );
                    assert!(
                        frontend::analyze(&bad)
                            .unwrap()
                            .iter()
                            .any(|d| d.code == "E0708"),
                        "{} -> {} bits={key:x}",
                        float.name,
                        target.name
                    );
                }
            }
        }
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn float_width_cast_edges_match_constant_interpreter_and_native() {
    let f32_max = f32::MAX as f64;
    let f32_smallest = f32::from_bits(1) as f64;
    let f32_normal = f32::MIN_POSITIVE as f64;
    let cases = [
        (
            "f64_to_f32_max",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f32_max,
        ),
        (
            "f64_to_f32_below_max",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            next_down_f64(f32_max),
        ),
        (
            "f64_to_f32_overflow",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            next_up_f64(f32_max),
        ),
        (
            "f64_to_f32_negative_overflow",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            -next_up_f64(f32_max),
        ),
        (
            "f64_to_f32_min_normal",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f32_normal,
        ),
        (
            "f64_to_f32_largest_subnormal",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            next_down_f64(f32_normal),
        ),
        (
            "f64_to_f32_min_subnormal",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f32_smallest,
        ),
        (
            "f64_to_f32_half_subnormal",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f32_smallest / 2.0,
        ),
        (
            "f64_to_f32_negative_subnormal",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            -f32_smallest,
        ),
        (
            "f64_to_f32_negative_zero",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            -0.0,
        ),
        (
            "f64_to_f32_nan",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f64::NAN,
        ),
        (
            "f64_to_f32_positive_infinity",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f64::INFINITY,
        ),
        (
            "f64_to_f32_negative_infinity",
            FloatType {
                name: "f64",
                bits: 64,
            },
            FloatType {
                name: "f32",
                bits: 32,
            },
            f64::NEG_INFINITY,
        ),
        (
            "f32_to_f64_min_subnormal",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            f32_smallest,
        ),
        (
            "f32_to_f64_min_normal",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            f32_normal,
        ),
        (
            "f32_to_f64_max",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            f32_max,
        ),
        (
            "f32_to_f64_negative_zero",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            -0.0,
        ),
        (
            "f32_to_f64_nan",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            f64::NAN,
        ),
        (
            "f32_to_f64_infinity",
            FloatType {
                name: "f32",
                bits: 32,
            },
            FloatType {
                name: "f64",
                bits: 64,
            },
            f64::INFINITY,
        ),
    ];
    for (name, source_type, target_type, value) in cases {
        let source = format!(
            "module float_width_cast; const SOURCE: {source_ty} = {value}; const EXPECTED: {target_ty} = SOURCE as {target_ty}; func main() -> i32 {{ let source: {source_ty} = SOURCE; let actual: {target_ty} = source as {target_ty}; println(EXPECTED); println(actual); return 0; }}",
            source_ty = source_type.name,
            target_ty = target_type.name,
            value = source_value(value, source_type),
        );
        let module =
            frontend::lower_to_ir(&source).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let interpreted = Interpreter::default()
            .execute(&module)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let lines = interpreted.output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2, "{name}: {:?}", interpreted.output);
        assert_eq!(lines[0], lines[1], "{name}: {:?}", interpreted.output);
        let native = common::run_native_case(name, &source)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(native.status.success(), "{name}: {native:?}");
        assert_eq!(
            String::from_utf8_lossy(&native.stdout)
                .lines()
                .collect::<Vec<_>>(),
            lines,
            "{name}"
        );
    }
}
