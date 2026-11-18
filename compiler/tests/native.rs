use jahe_compiler::{
    backend::{native::NativeBackend, BackendErrorKind},
    frontend,
    ir::{self, ExternalFunction, InstructionKind, Signature, Type},
};
use std::{fs, process::Command};

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
    let module = frontend::lower_to_ir(source).unwrap();
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
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "18\nnative ok\n"
    );
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn native_backend_rejects_unbound_external_functions_with_diagnostic() {
    let source = "module m; fn main() -> i32 { println(1); return 0; }";
    let mut module = frontend::lower_to_ir(source).unwrap();
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
    let mut module = frontend::lower_to_ir(source).unwrap();
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
