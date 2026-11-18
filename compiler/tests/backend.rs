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
