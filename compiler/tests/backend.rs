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
    frontend::lower_to_ir(source).unwrap()
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
