use std::{env, fs, process};

use jahe_compiler::{
    backend::{
        interpreter::{value::RuntimeValue, Interpreter},
        native::NativeBackend,
    },
    diagnostics::{render, render_ir},
    frontend,
    lexer::Lexer,
    VERSION,
};

fn usage() {
    eprintln!("usage: jahe [--version] <check|tokens|ast|ir|run|build> <file> | runtime-info");
}

fn run() -> Result<i32, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.as_slice() == ["--version"] || args.as_slice() == ["-V"] {
        println!("jahe {VERSION}");
        return Ok(0);
    }
    if args.as_slice() == ["runtime-info"] {
        println!("Runtime ABI version: {}", jahe_runtime::ABI_VERSION);
        println!(
            "Host architecture: {} ({})",
            env::consts::ARCH,
            env::consts::OS
        );
        println!("Capabilities: allocation, reallocation, deallocation, panic, typed println");
        println!("Runtime archive: static library, linked into native executables");
        return Ok(0);
    }
    if args.len() != 2 {
        usage();
        return Err("expected a command and source file".into());
    }
    let (command, filename) = (&args[0], &args[1]);
    let source = fs::read_to_string(filename)
        .map_err(|error| format!("could not read {filename}: {error}"))?;

    match command.as_str() {
        "tokens" => match Lexer::new(&source).tokenize() {
            Ok(tokens) => {
                for token in tokens {
                    println!("{}", token);
                }
            }
            Err(error) => return Err(render(filename, &source, &error)),
        },
        "ast" => match frontend::parse(&source) {
            Ok(module) => println!("{module:#?}"),
            Err(error) => return Err(render(filename, &source, &error)),
        },
        "check" => match frontend::analyze(&source) {
            Ok(errors) if errors.is_empty() => println!("{}: semantic checks passed", filename),
            Ok(errors) => {
                return Err(errors
                    .iter()
                    .map(|e| jahe_compiler::diagnostics::render_semantic(filename, &source, e))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            Err(error) => return Err(render(filename, &source, &error)),
        },
        "ir" => match frontend::lower_to_ir(&source) {
            Ok(module) => {
                let verified = jahe_compiler::ir::verify::Verifier
                    .verify(&module)
                    .map_err(|errors| {
                        errors
                            .iter()
                            .map(|e| render_ir(filename, &source, e))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })?;
                print!("{}", jahe_compiler::ir::format(verified.module()));
            }
            Err(errors) => {
                return Err(errors
                    .iter()
                    .map(|e| jahe_compiler::diagnostics::render_ir(filename, &source, e))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        },
        "run" => {
            let module = frontend::lower_to_ir(&source).map_err(|errors| {
                errors
                    .iter()
                    .map(|e| render_ir(filename, &source, e))
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            let interpreter = Interpreter::default();
            let verified = jahe_compiler::ir::verify::Verifier
                .verify(&module)
                .map_err(|errors| {
                    errors
                        .iter()
                        .map(|d| render_ir(filename, &source, d))
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            let result = interpreter.execute_verified(&verified).map_err(|error| {
                if !error.diagnostics.is_empty() {
                    error
                        .diagnostics
                        .iter()
                        .map(|d| render_ir(filename, &source, d))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else if let Some(runtime) = &error.runtime {
                    match runtime.span {
                        Some(span) => render(
                            filename,
                            &source,
                            &jahe_compiler::diagnostics::Diagnostic::new(
                                format!("[Runtime::{:?}] {}", runtime.kind, runtime.message),
                                span,
                            ),
                        ),
                        None => format!("runtime error: {}", runtime.message),
                    }
                } else {
                    format!("backend error: {error}")
                }
            })?;
            print!("{}", result.output);
            match result.return_value {
                None => return Ok(0),
                Some(RuntimeValue::I32(status)) if (0..=255).contains(&status) => {
                    return Ok(status)
                }
                Some(RuntimeValue::I32(status)) => {
                    return Err(format!(
                        "main returned {status}; process exit status must be between 0 and 255"
                    ))
                }
                Some(_) => return Err("main returned an unsupported process status value".into()),
            }
        }
        "build" => {
            let module = frontend::lower_to_ir(&source).map_err(|errors| {
                errors
                    .iter()
                    .map(|e| render_ir(filename, &source, e))
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            let verified = jahe_compiler::ir::verify::Verifier
                .verify(&module)
                .map_err(|errors| {
                    errors
                        .iter()
                        .map(|e| render_ir(filename, &source, e))
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            let input = std::path::Path::new(filename);
            let object = input.with_extension("obj");
            let executable = input.with_extension("exe");
            NativeBackend::default()
                .build_executable(&verified, &object, &executable)
                .map_err(|error| format!("native build failed: {error}"))?;
            println!("Built {} and {}", object.display(), executable.display());
        }
        _ => {
            usage();
            return Err(format!("unknown command: {command}"));
        }
    }
    Ok(0)
}

fn main() {
    match run() {
        Ok(status) => process::exit(status),
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    }
}
