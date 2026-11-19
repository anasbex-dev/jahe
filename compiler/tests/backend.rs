use jahe_compiler::{
    backend::{
        interpreter::{
            builtins::{BuiltinRegistry, HostContext, HostFunction},
            error::{RuntimeError, RuntimeErrorKind},
            value::RuntimeValue,
            ExecutionLimits, Interpreter,
        },
        Backend, BackendErrorKind,
    },
    frontend,
    ir::{self, ExternalFunction, InstructionKind, Signature, Terminator, Type},
};

fn compile(source: &str) -> ir::Module {
    frontend::lower_to_ir_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap()
}
fn run(source: &str) -> jahe_compiler::backend::interpreter::ExecutionResult {
    Interpreter::default().execute(&compile(source)).unwrap()
}

#[test]
fn interpreter_implements_backend_and_executes_hello() {
    let module = compile(include_str!("../../examples/hello.jahe"));
    let backend: &dyn Backend<
        Output = jahe_compiler::backend::interpreter::ExecutionResult,
        Error = jahe_compiler::backend::BackendError,
    > = &Interpreter::default();
    let verified = ir::verify::Verifier.verify(&module).unwrap();
    let result = backend.execute(&verified).unwrap();
    assert_eq!(result.output, "Hello from JAHE!\n");
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));
    assert!(result.steps > 0);
}

#[test]
fn interpreter_executes_character_values_and_equality() {
    let source = "module chars; func same(a: char, b: char) -> bool { return a == b; } func main() -> i32 { println('é'); if same('é', '\\u{E9}') { return 0; } else { return 1; } }";
    let result = run(source);
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));
    assert_eq!(result.output, "é\n");
}

#[test]
fn end_to_end_examples_cover_arithmetic_calls_branches_and_loops() {
    let arithmetic = run(include_str!("../../examples/arithmetic.jahe"));
    assert_eq!(arithmetic.output, "42\n");
    let functions = run(include_str!("../../examples/functions.jahe"));
    assert_eq!(functions.output, "7\n");
    let branches = run(include_str!("../../examples/control_flow.jahe"));
    assert_eq!(branches.output, "10\n");
    assert_eq!(branches.return_value, Some(RuntimeValue::I32(10)));
    let loops = run(include_str!("../../examples/loops.jahe"));
    assert_eq!(loops.output, "0\n1\n2\n");
}

#[test]
fn canonical_08_syntax_runs_with_inferred_locals_and_const_values() {
    let source = r#"
        module canonical;
        const ANSWER: i32 = 40 + 2;
        func add(a: i32, b: i32) -> i32 { return a + b; }
        func main() -> i32 {
            let base = 40;
            var value = add(base, 2);
            value = value + 1;
            println(ANSWER);
            return value - 43;
        }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));
    assert_eq!(result.output, "42\n");
}

#[test]
fn top_level_and_local_constants_can_reference_prior_constants() {
    let source = r#"module const_refs;
        const BASE: i32 = 40;
        const ANSWER: i32 = BASE + 2;
        func main() -> i32 {
            const LOCAL: i32 = ANSWER + 1;
            return LOCAL - 43;
        }
    "#;
    let result = run(source);
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));
}

#[test]
fn integer_remainder_runs_in_interpreter() {
    let module =
        frontend::lower_to_ir("module remainder; func main() -> i32 { return 17 % 5; }").unwrap();
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(2)));

    let by_zero =
        frontend::lower_to_ir("module remainder; func main() -> i32 { return 17 % 0; }").unwrap();
    let error = Interpreter::default().execute(&by_zero).unwrap_err();
    assert!(error.to_string().contains("division by zero"));

    let signed_overflow = frontend::lower_to_ir(
        "module remainder; func main() -> i32 { return (-2147483648) % -1; }",
    )
    .unwrap();
    assert!(Interpreter::default().execute(&signed_overflow).is_err());
}

#[test]
fn explicit_numeric_casts_run_and_reject_out_of_range_values() {
    let source = r#"module casts; func main() -> i32 {
        let wide = 100 as i64;
        let precise = wide as f64;
        let truncated = (precise + 3.9) as i32;
        const THREE: i32 = 3.9 as i32;
        return truncated + THREE;
    }"#;
    let module = frontend::lower_to_ir(source).unwrap();
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(106)));

    let invalid = frontend::lower_to_ir(
        "module casts; func main() -> i32 { let wide = 2147483648 as i64; return wide as i32; }",
    )
    .unwrap();
    let error = Interpreter::default().execute(&invalid).unwrap_err();
    assert_eq!(error.kind, BackendErrorKind::Runtime);
    assert!(error.to_string().contains("cannot be represented"));

    let compile_time_invalid = frontend::lower_to_ir(
        "module casts; func main() -> i32 { let value = 2147483648 as i32; return value; }",
    )
    .unwrap_err();
    assert!(compile_time_invalid
        .iter()
        .any(|diagnostic| diagnostic.code == "E0304"));
}

#[test]
fn integer_bitwise_and_shift_operations_run_with_checked_counts() {
    let source = "module bits; func main() -> i32 { let value = (1 << 3) | (16 >> 2); return value + (6 & 3) + (1 ^ 3); }";
    let module = frontend::lower_to_ir(source).unwrap();
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(16)));

    let invalid =
        frontend::lower_to_ir("module bits; func main() -> i32 { return 1 << 32; }").unwrap();
    let error = Interpreter::default().execute(&invalid).unwrap_err();
    assert!(error.to_string().contains("shift count must be between"));

    let overflow =
        frontend::lower_to_ir("module bits; func main() -> i32 { return 1073741824 << 1; }")
            .unwrap();
    assert!(Interpreter::default().execute(&overflow).is_err());

    let signed_right =
        frontend::lower_to_ir("module bits; func main() -> i32 { return -8 >> 2; }").unwrap();
    let result = Interpreter::default().execute(&signed_right).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(-2)));
}

#[test]
fn signed_i32_minimum_literal_is_representable() {
    let module =
        frontend::lower_to_ir("module min_value; func main() -> i32 { return -2147483648; }")
            .unwrap();
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(i32::MIN)));
}

#[test]
fn signed_i64_minimum_literal_is_representable() {
    let module = frontend::lower_to_ir(
        "module min_value; func value() -> i64 { return -9223372036854775808; } func main() -> i32 { return 0; }",
    )
    .unwrap();
    let result = Interpreter::default()
        .execute_function(&module, "value", Vec::new())
        .unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I64(i64::MIN)));
}

#[test]
fn integer_widths_and_unsigned_values_execute_in_interpreter() {
    let source = r#"
        module integer_widths;
        func min_i8() -> i8 { return -128; }
        func max_i16() -> i16 { return 32767; }
        func add_u8(a: u8, b: u8) -> u8 { return a + b; }
        func max_u64() -> u64 { return 18446744073709551615; }
        func signed_shift() -> i16 { let value: i16 = -8; return value >> 2; }
        func unsigned_shift() -> u8 { let value: u8 = 128; return value >> 7; }
        func unsigned_rem() -> u8 { return 255 % 10; }
        func unsigned_div() -> u64 { return 18446744073709551615 / 3; }
        func wide_add() -> i128 { return 170141183460469231731687303715884105726 + 1; }
        func wide_signed_div() -> i128 { return -120 / 7; }
        func wide_signed_rem() -> i128 { return -120 % 7; }
        func wide_unsigned_div() -> u128 { return 340282366920938463463374607431768211455 / 3; }
        func underflow() -> u8 { let value: u8 = 0; return value - 1; }
        func main() -> i32 { return 0; }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let interpreter = Interpreter::default();
    assert_eq!(
        interpreter
            .execute_function(&module, "min_i8", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 0x80,
            ty: Type::I8
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "max_i16", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 0x7fff,
            ty: Type::I16
        })
    );
    assert_eq!(
        interpreter
            .execute_function(
                &module,
                "add_u8",
                vec![
                    RuntimeValue::Integer {
                        bits: 200,
                        ty: Type::U8
                    },
                    RuntimeValue::Integer {
                        bits: 55,
                        ty: Type::U8
                    },
                ]
            )
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 255,
            ty: Type::U8
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "max_u64", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: u64::MAX as u128,
            ty: Type::U64
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "signed_shift", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 0xfffe,
            ty: Type::I16
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "unsigned_shift", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 1,
            ty: Type::U8
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "unsigned_rem", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 5,
            ty: Type::U8
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "unsigned_div", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: (u64::MAX / 3) as u128,
            ty: Type::U64
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "wide_add", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: i128::MAX as u128,
            ty: Type::I128
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "wide_signed_div", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: (-17i128) as u128,
            ty: Type::I128
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "wide_signed_rem", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: (-1i128) as u128,
            ty: Type::I128
        })
    );
    assert_eq!(
        interpreter
            .execute_function(&module, "wide_unsigned_div", Vec::new())
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: u128::MAX / 3,
            ty: Type::U128
        })
    );
    let overflow = interpreter
        .execute_function(&module, "underflow", Vec::new())
        .unwrap_err();
    assert_eq!(
        overflow.runtime.unwrap().kind,
        jahe_compiler::backend::interpreter::error::RuntimeErrorKind::IntegerOverflow
    );
}

#[test]
fn all_integer_literal_boundaries_execute_in_interpreter() {
    let source = r#"
        module all_integer_ranges;
        const MAX_U128: u128 = 340282366920938463463374607431768211455;
        const MIN_I128: i128 = -170141183460469231731687303715884105728;
        func min_i8() -> i8 { return -128; }
        func max_i8() -> i8 { return 127; }
        func min_i16() -> i16 { return -32768; }
        func max_i16() -> i16 { return 32767; }
        func min_i32() -> i32 { return -2147483648; }
        func max_i32() -> i32 { return 2147483647; }
        func min_i64() -> i64 { return -9223372036854775808; }
        func max_i64() -> i64 { return 9223372036854775807; }
        func min_i128() -> i128 { return -170141183460469231731687303715884105728; }
        func max_i128() -> i128 { return 170141183460469231731687303715884105727; }
        func min_isize() -> isize { return -9223372036854775808; }
        func max_isize() -> isize { return 9223372036854775807; }
        func max_u8() -> u8 { return 255; }
        func max_u16() -> u16 { return 65535; }
        func max_u32() -> u32 { return 4294967295; }
        func max_u64() -> u64 { return 18446744073709551615; }
        func max_u128() -> u128 { return MAX_U128; }
        func min_i128_const() -> i128 { return MIN_I128; }
        func max_usize() -> usize { return 18446744073709551615; }
        func main() -> i32 { return 0; }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let interpreter = Interpreter::default();
    for (name, expected) in [
        (
            "min_i8",
            RuntimeValue::Integer {
                bits: 0x80,
                ty: Type::I8,
            },
        ),
        (
            "max_i8",
            RuntimeValue::Integer {
                bits: 0x7f,
                ty: Type::I8,
            },
        ),
        (
            "min_i16",
            RuntimeValue::Integer {
                bits: 0x8000,
                ty: Type::I16,
            },
        ),
        (
            "max_i16",
            RuntimeValue::Integer {
                bits: 0x7fff,
                ty: Type::I16,
            },
        ),
        ("min_i32", RuntimeValue::I32(i32::MIN)),
        ("max_i32", RuntimeValue::I32(i32::MAX)),
        ("min_i64", RuntimeValue::I64(i64::MIN)),
        ("max_i64", RuntimeValue::I64(i64::MAX)),
        (
            "min_i128",
            RuntimeValue::Integer {
                bits: 1u128 << 127,
                ty: Type::I128,
            },
        ),
        (
            "max_i128",
            RuntimeValue::Integer {
                bits: i128::MAX as u128,
                ty: Type::I128,
            },
        ),
        (
            "min_isize",
            RuntimeValue::Integer {
                bits: 1u128 << 63,
                ty: Type::ISize,
            },
        ),
        (
            "max_isize",
            RuntimeValue::Integer {
                bits: i64::MAX as u128,
                ty: Type::ISize,
            },
        ),
        (
            "max_u8",
            RuntimeValue::Integer {
                bits: u8::MAX as u128,
                ty: Type::U8,
            },
        ),
        (
            "max_u16",
            RuntimeValue::Integer {
                bits: u16::MAX as u128,
                ty: Type::U16,
            },
        ),
        (
            "max_u32",
            RuntimeValue::Integer {
                bits: u32::MAX as u128,
                ty: Type::U32,
            },
        ),
        (
            "max_u64",
            RuntimeValue::Integer {
                bits: u64::MAX as u128,
                ty: Type::U64,
            },
        ),
        (
            "max_u128",
            RuntimeValue::Integer {
                bits: u128::MAX,
                ty: Type::U128,
            },
        ),
        (
            "min_i128_const",
            RuntimeValue::Integer {
                bits: 1u128 << 127,
                ty: Type::I128,
            },
        ),
        (
            "max_usize",
            RuntimeValue::Integer {
                bits: u64::MAX as u128,
                ty: Type::USize,
            },
        ),
    ] {
        let output = interpreter
            .execute_function(&module, name, Vec::new())
            .unwrap();
        assert_eq!(output.return_value, Some(expected), "function {name}");
    }
}

#[test]
fn integer_casts_preserve_sign_and_check_narrowing() {
    let source = r#"
        module integer_casts;
        func widen(value: i8) -> i16 { return value as i16; }
        func unsigned_widen(value: u8) -> u32 { return value as u32; }
        func narrow(value: u16) -> u8 { return value as u8; }
        func negative_to_unsigned(value: i16) -> u8 { return value as u8; }
        func main() -> i32 { return 0; }
    "#;
    let module = frontend::lower_to_ir(source).unwrap();
    let interpreter = Interpreter::default();
    assert_eq!(
        interpreter
            .execute_function(
                &module,
                "widen",
                vec![RuntimeValue::Integer {
                    bits: 0xf4,
                    ty: Type::I8
                }]
            )
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 0xfff4,
            ty: Type::I16
        })
    );
    assert_eq!(
        interpreter
            .execute_function(
                &module,
                "unsigned_widen",
                vec![RuntimeValue::Integer {
                    bits: 255,
                    ty: Type::U8
                }]
            )
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 255,
            ty: Type::U32
        })
    );
    assert_eq!(
        interpreter
            .execute_function(
                &module,
                "narrow",
                vec![RuntimeValue::Integer {
                    bits: 255,
                    ty: Type::U16
                }]
            )
            .unwrap()
            .return_value,
        Some(RuntimeValue::Integer {
            bits: 255,
            ty: Type::U8
        })
    );
    let invalid = interpreter
        .execute_function(
            &module,
            "narrow",
            vec![RuntimeValue::Integer {
                bits: 256,
                ty: Type::U16,
            }],
        )
        .unwrap_err();
    assert_eq!(
        invalid.runtime.unwrap().kind,
        jahe_compiler::backend::interpreter::error::RuntimeErrorKind::InvalidCast
    );
    let invalid = interpreter
        .execute_function(
            &module,
            "negative_to_unsigned",
            vec![RuntimeValue::Integer {
                bits: 0xffff,
                ty: Type::I16,
            }],
        )
        .unwrap_err();
    assert_eq!(
        invalid.runtime.unwrap().kind,
        jahe_compiler::backend::interpreter::error::RuntimeErrorKind::InvalidCast
    );
}

#[test]
fn executes_phi_nodes_and_nested_function_frames() {
    let result = run("module m; fn add(a: i32, b: i32) -> i32 { return a + b; } fn main() -> i32 { let mut n: i32 = 0; while n < 3 { n = add(n, 1); } if n == 3 { return add(n, 4); } else { return 0; } }");
    assert_eq!(result.return_value, Some(RuntimeValue::I32(7)));
}

#[test]
fn verifier_blocks_invalid_ir_from_execution() {
    let mut module = compile("module m; fn main() -> i32 { return 0; }");
    module.functions[0].blocks[0].terminator = Some(Terminator::Return(Some(ir::Value {
        id: ir::ValueId(777),
        ty: Type::I32,
        span: module.functions[0].span,
    })));
    let error = Interpreter::default().execute(&module).unwrap_err();
    assert_eq!(error.kind, BackendErrorKind::Verification);
    assert!(!error.diagnostics.is_empty());
}

#[test]
fn runtime_reports_division_overflow_and_execution_limits() {
    let interpreter = Interpreter::default();
    let division = compile("module m; fn main() -> i32 { let x: i32 = 1 / 0; return x; }");
    assert_eq!(
        interpreter
            .execute(&division)
            .unwrap_err()
            .runtime
            .unwrap()
            .kind,
        RuntimeErrorKind::DivisionByZero
    );
    let overflow = compile("module m; fn main() -> i32 { let x: i32 = 2147483647 + 1; return x; }");
    assert_eq!(
        interpreter
            .execute(&overflow)
            .unwrap_err()
            .runtime
            .unwrap()
            .kind,
        RuntimeErrorKind::IntegerOverflow
    );
    let infinite = compile("module m; fn loop_forever() { while true { } }");
    let limited = Interpreter::new(ExecutionLimits {
        max_steps: 20,
        max_call_depth: 8,
    });
    assert_eq!(
        limited
            .execute_function(&infinite, "loop_forever", vec![])
            .unwrap_err()
            .runtime
            .unwrap()
            .kind,
        RuntimeErrorKind::StepLimitExceeded
    );
}

#[test]
fn call_depth_is_bounded_and_run_state_is_isolated_after_failure() {
    let recursive = compile("module m; fn recurse() -> i32 { return recurse(); }");
    let limited = Interpreter::new(ExecutionLimits {
        max_steps: 100,
        max_call_depth: 3,
    });
    assert_eq!(
        limited
            .execute_function(&recursive, "recurse", vec![])
            .unwrap_err()
            .runtime
            .unwrap()
            .kind,
        RuntimeErrorKind::CallDepthExceeded
    );

    let bad = compile("module m; fn main() -> i32 { let n: i32 = 1 / 0; return n; }");
    let good = compile(include_str!("../../examples/hello.jahe"));
    assert!(limited.execute(&bad).is_err());
    assert_eq!(limited.execute(&good).unwrap().output, "Hello from JAHE!\n");
    assert_eq!(limited.execute(&good).unwrap().output, "Hello from JAHE!\n");
}

#[test]
fn host_functions_are_registered_and_runtime_errors_are_structured() {
    struct Echo;
    impl HostFunction for Echo {
        fn signature(&self) -> Signature {
            Signature::exact([Type::I32], Type::I32)
        }
        fn call(
            &self,
            args: &[RuntimeValue],
            context: &mut HostContext,
            _span: jahe_compiler::lexer::token::Span,
        ) -> Result<Option<RuntimeValue>, RuntimeError> {
            context.output.push_str("host called\n");
            Ok(args.first().cloned())
        }
    }
    let mut module = compile("module m; fn main() -> i32 { println(7); return 0; }");
    let call = module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    let argument = match &call.kind {
        InstructionKind::Call { args, .. } => args[0].clone(),
        _ => unreachable!(),
    };
    call.kind = InstructionKind::Call {
        function: "host_echo".into(),
        args: vec![argument],
    };
    module.externals.push(ExternalFunction::new(
        "host_echo",
        Signature::exact([Type::I32], Type::I32),
        call.span,
    ));
    call.result = Some(ir::Value {
        id: ir::ValueId(88),
        ty: Type::I32,
        span: call.span,
    });
    let mut registry = BuiltinRegistry::with_standard();
    registry.register("host_echo", Echo).unwrap();
    assert!(registry.register("host_echo", Echo).is_err());
    let interpreter = Interpreter::with_builtins(ExecutionLimits::default(), registry);
    let result = interpreter.execute(&module).unwrap();
    assert_eq!(result.output, "host called\n");

    struct BadReturn;
    impl HostFunction for BadReturn {
        fn signature(&self) -> Signature {
            Signature::exact([Type::I32], Type::I32)
        }
        fn call(
            &self,
            _args: &[RuntimeValue],
            _context: &mut HostContext,
            _span: jahe_compiler::lexer::token::Span,
        ) -> Result<Option<RuntimeValue>, RuntimeError> {
            Ok(Some(RuntimeValue::Bool(true)))
        }
    }
    let mut bad_return_registry = BuiltinRegistry::with_standard();
    bad_return_registry
        .register("host_echo", BadReturn)
        .unwrap();
    let bad_return = Interpreter::with_builtins(ExecutionLimits::default(), bad_return_registry)
        .execute(&module)
        .unwrap_err();
    assert_eq!(
        bad_return.runtime.unwrap().kind,
        RuntimeErrorKind::ExternalReturnMismatch
    );

    struct Fails;
    impl HostFunction for Fails {
        fn signature(&self) -> Signature {
            Signature::exact([Type::I32], Type::I32)
        }
        fn call(
            &self,
            _args: &[RuntimeValue],
            _context: &mut HostContext,
            span: jahe_compiler::lexer::token::Span,
        ) -> Result<Option<RuntimeValue>, RuntimeError> {
            Err(RuntimeError::new(
                RuntimeErrorKind::HostFunction,
                "host failure",
                Some(span),
            ))
        }
    }
    let mut failing_registry = BuiltinRegistry::with_standard();
    failing_registry.register("host_echo", Fails).unwrap();
    let host_error = Interpreter::with_builtins(ExecutionLimits::default(), failing_registry)
        .execute(&module)
        .unwrap_err();
    assert_eq!(
        host_error.runtime.unwrap().kind,
        RuntimeErrorKind::HostFunction
    );

    struct WrongSignature;
    impl HostFunction for WrongSignature {
        fn signature(&self) -> Signature {
            Signature::exact([Type::I64], Type::I32)
        }
        fn call(
            &self,
            _args: &[RuntimeValue],
            _context: &mut HostContext,
            _span: jahe_compiler::lexer::token::Span,
        ) -> Result<Option<RuntimeValue>, RuntimeError> {
            Ok(Some(RuntimeValue::I32(0)))
        }
    }
    let mut wrong_registry = BuiltinRegistry::with_standard();
    wrong_registry
        .register("host_echo", WrongSignature)
        .unwrap();
    let wrong = Interpreter::with_builtins(ExecutionLimits::default(), wrong_registry)
        .execute(&module)
        .unwrap_err();
    assert_eq!(wrong.kind, BackendErrorKind::ExternalSignatureMismatch);

    let unresolved =
        Interpreter::with_builtins(ExecutionLimits::default(), BuiltinRegistry::default())
            .execute(&module)
            .unwrap_err();
    assert_eq!(
        unresolved.kind,
        BackendErrorKind::ExternalFunctionUnresolved
    );

    let mut unreachable = compile("module m; fn main() { return; }");
    unreachable.functions[0].blocks[0].terminator = Some(Terminator::Unreachable);
    let error = interpreter.execute(&unreachable).unwrap_err();
    assert_eq!(
        error.runtime.unwrap().kind,
        RuntimeErrorKind::InvalidControlFlow
    );
}

#[test]
fn interpreter_preserves_float_types_and_values() {
    let module = compile("module m; fn multiply(a: f32, b: f32) -> f32 { return a * b; }");
    let result = Interpreter::default()
        .execute_function(
            &module,
            "multiply",
            vec![RuntimeValue::F32(1.5), RuntimeValue::F32(2.0)],
        )
        .unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::F32(3.0)));
}

#[test]
fn interpreter_float_ieee_special_values_and_runtime_negation() {
    let source = "module ieee; func negate(value: f64) -> f64 { return -value; } func main() -> i32 { let neg_zero = negate(0.0); let infinity = 1.0 / neg_zero; let nan = infinity / infinity; if nan != nan { if neg_zero == 0.0 { if infinity < 0.0 { return 1; } } } return 0; }";
    let result = run(source);
    assert_eq!(result.return_value, Some(RuntimeValue::I32(1)));
}

#[test]
fn bool_not_and_logical_operators_short_circuit() {
    let source = r#"module logic;
        const OK: bool = !(false || false);
        func side_effect() -> bool { println("unexpected"); return true; }
        func main() -> i32 {
            let a = false && side_effect();
            let b = true || side_effect();
            if OK && !a && b { return 0; }
            return 1;
        }
    "#;
    let result = run(source);
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));
    assert_eq!(result.output, "");
}

#[test]
fn never_functions_diverge_and_coerce_in_returning_control_flow() {
    let source = "module never_control; func spin() -> never { while true {} } func choose(flag: bool) -> i32 { if flag { spin(); } else { return 0; } } func main() -> i32 { return choose(false); }";
    let module = compile(source);
    let result = Interpreter::default().execute(&module).unwrap();
    assert_eq!(result.return_value, Some(RuntimeValue::I32(0)));

    let spin = compile("module spin_limit; func spin() -> never { while true {} } func main() -> i32 { return 0; }");
    let limited = Interpreter::new(ExecutionLimits {
        max_steps: 32,
        max_call_depth: 8,
    });
    let error = limited
        .execute_function(&spin, "spin", Vec::new())
        .unwrap_err();
    assert!(error.to_string().contains("execution exceeded 32 steps"));
}

#[test]
fn never_function_without_divergence_is_rejected() {
    let errors = frontend::analyze("module never_bad; func bad() -> never {}");
    assert!(errors.unwrap().iter().any(|error| error.code == "E0601"));
}
