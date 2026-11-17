use std::{env, fs, process};

use jahe_compiler::{diagnostics::render, frontend, lexer::Lexer, VERSION};

fn usage() {
    eprintln!("usage: jahe [--version] <check|tokens|ast|ir> <file>");
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.as_slice() == ["--version"] || args.as_slice() == ["-V"] {
        println!("jahe {VERSION}");
        return Ok(());
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
            Ok(module) => print!("{}", jahe_compiler::ir::format(&module)),
            Err(errors) => {
                return Err(errors
                    .iter()
                    .map(|e| jahe_compiler::diagnostics::render_ir(filename, &source, e))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        },
        _ => {
            usage();
            return Err(format!("unknown command: {command}"));
        }
    }
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        process::exit(1);
    }
}
