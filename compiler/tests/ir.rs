use jahe_compiler::{
    frontend,
    ir::{self, ExternalFunction, InstructionKind, Signature, Terminator, Type},
};

fn lower(source: &str) -> ir::Module {
    frontend::lower_to_ir_with_mode(source, jahe_compiler::parser::SyntaxMode::Legacy).unwrap()
}

#[test]
fn lowers_functions_parameters_and_return_values_with_types() {
    let module = lower("module m; fn add(a: i32, b: i32) -> i32 { return a + b; }");
    let function = &module.functions[0];
    assert_eq!(function.params.len(), 2);
    assert_eq!(function.params[0].value.ty, Type::I32);
    assert_eq!(function.return_type, Type::I32);
    assert!(matches!(
        function.blocks[0].instructions[0].kind,
        InstructionKind::Binary { .. }
    ));
    assert!(matches!(
        function.blocks[0].terminator,
        Some(Terminator::Return(Some(_)))
    ));
}

#[test]
fn pointer_sized_integers_require_an_explicit_supported_target() {
    let source = "module target; func value() -> usize { return 1; }";
    let unsupported = jahe_compiler::frontend::lower_to_ir_for_target(
        source,
        jahe_compiler::target::TargetSpec {
            triple: "aarch64-unknown-linux-gnu",
            pointer_width: 64,
        },
    )
    .unwrap_err();
    assert!(unsupported.iter().any(|d| d.code == "E0901"));

    let supported = jahe_compiler::frontend::lower_to_ir_for_target(
        source,
        jahe_compiler::target::TargetSpec::WINDOWS_X86_64_MSVC,
    )
    .unwrap();
    assert_eq!(
        supported.functions[0].return_type,
        jahe_compiler::ir::Type::USize
    );
}

#[test]
fn lowers_variables_literals_comparisons_calls_and_locations() {
    let module = lower("module m; fn is_ok(x: f64) -> bool { let y: f64 = x + 1.5; return y >= 2.0; } fn main() { let ok: bool = is_ok(3.0); println(ok); }");
    let function = &module.functions[0];
    assert_eq!(function.span.line, 1);
    assert!(function.blocks[0].instructions.iter().any(|i| matches!(
        i.kind,
        InstructionKind::Binary {
            op: ir::BinaryOp::Add,
            ..
        }
    ) && i
        .result
        .as_ref()
        .is_some_and(|v| v.ty == Type::F64)));
    assert!(function.blocks[0].instructions.iter().any(|i| matches!(
        i.kind,
        InstructionKind::Binary {
            op: ir::BinaryOp::GreaterEqual,
            ..
        }
    ) && i
        .result
        .as_ref()
        .is_some_and(|v| v.ty == Type::Bool)));
    let main = &module.functions[1];
    assert!(main.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(&i.kind, InstructionKind::Call { function, .. } if function == "is_ok")));
    assert!(main.blocks[0].instructions.iter().any(
        |i| matches!(&i.kind, InstructionKind::Call { function, .. } if function == "println")
    ));
    assert!(main.blocks[0].instructions.iter().all(|i| i.span.line == 1));
}

#[test]
fn lowers_constant_kinds_and_prints_backend_neutral_text() {
    let module = lower("module m; fn values() { let n: i64 = 2147483648; let f: f64 = 2.5; let s: str = \"x\"; let b: bool = true; }");
    assert!(module.functions[0].blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(
            i.kind,
            InstructionKind::Constant(ir::Constant::Integer(2147483648))
        )));
    let text = ir::format(&module);
    assert!(text.contains("fn @values"));
    assert!(text.contains("const \"x\""));
    assert!(text.contains("ret void"));
}

#[test]
fn refuses_semantically_invalid_or_unlowerable_programs() {
    let mismatch = frontend::lower_to_ir_with_mode(
        "module m; fn f() -> i32 { return true; }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert!(!mismatch.is_empty());
    let missing_return = frontend::lower_to_ir_with_mode(
        "module m; fn f() -> i32 { }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert!(missing_return.iter().any(|e| e.code == "E0601"));
    let unreachable = frontend::lower_to_ir_with_mode(
        "module m; fn f() { return; println(1); }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert!(unreachable.iter().any(|e| e.code == "E0602"));
}

#[test]
fn validator_rejects_unknown_function_and_bad_return_type() {
    let mut module = lower("module m; fn f() -> i32 { return 1; }");
    let block = &mut module.functions[0].blocks[0];
    block.instructions.clear();
    block.terminator = Some(Terminator::Return(None));
    let errors = ir::validate::validate(&module).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "IR0021"));

    let mut module = lower("module m; fn f() { println(1); }");
    module.functions[0].blocks[0].instructions[1] = ir::Instruction {
        result: None,
        kind: InstructionKind::Call {
            function: "missing".into(),
            args: vec![],
        },
        span: module.functions[0].span,
    };
    let errors = ir::validate::validate(&module).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "IR0017"));
}

#[test]
fn verifier_checks_external_signatures_and_declaration_conflicts() {
    let mut module = lower("module m; fn main() { println(1); }");
    let call = module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|instruction| matches!(&instruction.kind, InstructionKind::Call { .. }))
        .unwrap();
    if let InstructionKind::Call { function, .. } = &mut call.kind {
        *function = "host".into();
    }
    // A valid explicit external declaration resolves the call.
    module.externals.push(ExternalFunction::new(
        "host",
        Signature::exact([Type::I32], Type::Void),
        module.functions[0].span,
    ));
    ir::verify::Verifier.verify(&module).unwrap();

    module.externals.push(ExternalFunction::new(
        "host",
        Signature::exact([Type::I64], Type::Void),
        module.functions[0].span,
    ));
    assert!(ir::validate::validate(&module)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0041"));

    let mut missing = lower("module m; fn main() { println(1); }");
    missing.externals.retain(|e| e.name != "println");
    assert!(ir::validate::validate(&missing)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0017"));
}

#[test]
fn lowers_if_else_with_branches_and_mutable_value_phi() {
    let module = lower("module m; fn choose(flag: bool) -> i32 { let mut value: i32 = 10; if flag { value = 1; } else { value = 0; } return value; }");
    let f = &module.functions[0];
    assert!(matches!(
        f.blocks[0].terminator,
        Some(Terminator::ConditionalBranch { .. })
    ));
    let merge = f
        .blocks
        .iter()
        .find(|b| b.name.starts_with("if.merge"))
        .unwrap();
    let phi = merge
        .instructions
        .iter()
        .find(|i| matches!(i.kind, InstructionKind::Phi { .. }))
        .unwrap();
    assert!(matches!(&phi.kind, InstructionKind::Phi { incoming } if incoming.len() == 2));
    assert!(matches!(
        merge.terminator,
        Some(Terminator::Return(Some(_)))
    ));
}

#[test]
fn lowers_while_to_loop_header_phi_and_checks_return_paths() {
    let module = lower("module m; fn count() -> i32 { let mut counter: i32 = 0; while counter < 10 { counter = counter + 1; } return counter; }");
    let f = &module.functions[0];
    let header = f
        .blocks
        .iter()
        .find(|b| b.name.starts_with("while.header"))
        .unwrap();
    assert!(header
        .instructions
        .iter()
        .any(|i| matches!(&i.kind, InstructionKind::Phi { incoming } if incoming.len() == 2)));
    assert!(f
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Some(Terminator::ConditionalBranch { .. }))));

    let missing = frontend::lower_to_ir_with_mode(
        "module m; fn maybe(flag: bool) -> i32 { if flag { return 1; } }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert!(missing.iter().any(|e| e.code == "E0601"));
    let complete =
        lower("module m; fn both(flag: bool) -> i32 { if flag { return 1; } else { return 0; } }");
    assert_eq!(
        complete.functions[0]
            .blocks
            .iter()
            .filter(|b| matches!(b.terminator, Some(Terminator::Return(_))))
            .count(),
        2
    );
}

#[test]
fn validator_rejects_bad_targets_phi_inputs_and_use_order() {
    let mut m = lower("module m; fn f() -> i32 { return 1; }");
    m.functions[0].blocks[0].terminator = Some(Terminator::Branch(ir::BlockId(99)));
    assert!(ir::validate::validate(&m)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0023"));

    let mut m = lower("module m; fn f() -> i32 { return 1; }");
    let f = &mut m.functions[0];
    let span = f.span;
    let other = ir::BasicBlock {
        id: ir::BlockId(1),
        name: "other".into(),
        instructions: vec![],
        terminator: Some(Terminator::Return(Some(
            f.blocks[0].instructions[0].result.clone().unwrap(),
        ))),
        span,
    };
    f.blocks[0].terminator = Some(Terminator::Branch(ir::BlockId(1)));
    f.blocks.push(other);
    f.blocks[0].instructions.push(ir::Instruction {
        result: Some(ir::Value {
            id: ir::ValueId(50),
            ty: Type::I32,
            span,
        }),
        kind: InstructionKind::Phi { incoming: vec![] },
        span,
    });
    let errs = ir::validate::validate(&m).unwrap_err();
    assert!(errs.iter().any(|e| e.code == "IR0031"));
}

#[test]
fn expression_spans_survive_ast_lowering() {
    let m = lower("module m;\nfn f() -> i32 {\n return 123;\n}");
    let literal = &m.functions[0].blocks[0].instructions[0];
    assert_eq!((literal.span.line, literal.span.column), (3, 9));
}

#[test]
fn validator_checks_ssa_undefined_duplicate_and_definition_order() {
    let mut undefined = lower("module m; fn f() -> i32 { return 1; }");
    undefined.functions[0].blocks[0].terminator = Some(Terminator::Return(Some(ir::Value {
        id: ir::ValueId(999),
        ty: Type::I32,
        span: undefined.functions[0].span,
    })));
    assert!(ir::validate::validate(&undefined)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0019"));

    let mut duplicate = lower("module m; fn f() -> i32 { return 1 + 2; }");
    let block = &mut duplicate.functions[0].blocks[0];
    let first = block.instructions[0].result.clone().unwrap();
    block.instructions[1].result.as_mut().unwrap().id = first.id;
    assert!(ir::validate::validate(&duplicate)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0011"));

    let mut order = lower("module m; fn f() -> i32 { return 1 + 2; }");
    let block = &mut order.functions[0].blocks[0];
    let binary_result = block.instructions[2].result.clone().unwrap();
    if let InstructionKind::Binary { left, .. } = &mut block.instructions[2].kind {
        *left = binary_result;
    }
    assert!(ir::validate::validate(&order)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0025"));
}

#[test]
fn builder_rejects_invalid_instruction_order_and_duplicate_terminators() {
    let span = jahe_compiler::lexer::token::Span { line: 1, column: 1 };
    let mut builder = ir::builder::FunctionBuilder::new("f", Type::Void, span);
    builder.return_value(None, span).unwrap();
    let after_terminator = builder
        .emit(
            InstructionKind::Constant(ir::Constant::Integer(1)),
            Some(Type::I32),
            span,
        )
        .unwrap_err();
    assert_eq!(after_terminator.code, "IRB004");
    let duplicate = builder.return_value(None, span).unwrap_err();
    assert_eq!(duplicate.code, "IRB005");
}

#[test]
fn lowers_nested_if_and_nested_loops() {
    let nested_if = lower("module m; fn choose(a: bool, b: bool) -> i32 { let mut x: i32 = 0; if a { if b { x = 1; } else { x = 2; } } else { x = 3; } return x; }");
    assert!(
        nested_if.functions[0]
            .blocks
            .iter()
            .filter(|b| b
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::Phi { .. })))
            .count()
            >= 2
    );

    let nested_loops = lower("module m; fn count() -> i32 { let mut i: i32 = 0; let mut j: i32 = 0; while i < 2 { j = 0; while j < 2 { j = j + 1; } i = i + 1; } return i; }");
    assert_eq!(
        nested_loops.functions[0]
            .blocks
            .iter()
            .filter(|b| b
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::Phi { .. })))
            .count(),
        2
    );
}

#[test]
fn validator_checks_missing_terminators_branch_condition_and_phi_types() {
    let mut missing = lower("module m; fn f() { }");
    missing.functions[0].blocks[0].terminator = None;
    assert!(ir::validate::validate(&missing)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0020"));

    let mut branch = lower("module m; fn f() { if true { } }");
    if let Some(Terminator::ConditionalBranch { condition, .. }) =
        &mut branch.functions[0].blocks[0].terminator
    {
        condition.ty = Type::I32;
    }
    assert!(ir::validate::validate(&branch)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0024"));

    let mut phi = lower("module m; fn f() { let mut n: i32 = 0; while n < 2 { n = n + 1; } }");
    let header = phi.functions[0]
        .blocks
        .iter_mut()
        .find(|b| b.name.starts_with("while.header"))
        .unwrap();
    if let InstructionKind::Phi { incoming } = &mut header.instructions[0].kind {
        incoming[0].1.ty = Type::Bool;
    }
    assert!(ir::validate::validate(&phi)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0032"));
}

#[test]
fn validator_rejects_non_dominating_values_and_nonreturning_loop_exit_paths() {
    let mut m = lower("module m; fn f(flag: bool) -> i32 { let mut value: i32 = 0; if flag { value = 1; } else { value = 2; } return value; }");
    let branch_value = m.functions[0]
        .blocks
        .iter()
        .find(|b| b.name.starts_with("if.then"))
        .unwrap()
        .instructions[0]
        .result
        .clone()
        .unwrap();
    let merge = m.functions[0]
        .blocks
        .iter_mut()
        .find(|b| b.name.starts_with("if.merge"))
        .unwrap();
    merge.instructions.clear();
    merge.terminator = Some(Terminator::Return(Some(branch_value)));
    assert!(ir::validate::validate(&m)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "IR0026"));

    let errors = frontend::lower_to_ir_with_mode(
        "module m; fn only_loops() -> i32 { while false { return 1; } }",
        jahe_compiler::parser::SyntaxMode::Legacy,
    )
    .unwrap_err();
    assert!(errors.iter().any(|e| e.code == "E0601"));
}
