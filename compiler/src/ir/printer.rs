use super::{Constant, InstructionKind, Module, Terminator};
use std::fmt::Write;
pub fn format(module: &Module) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "module {}", module.name);
    for f in &module.functions {
        let ps = f
            .params
            .iter()
            .map(|p| format!("%{}: {} {}", p.value.id.0, p.value.ty, p.name))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(o, "\nfn @{}({ps}) -> {} {{", f.name, f.return_type);
        for b in &f.blocks {
            let _ = writeln!(o, "{}:", b.name);
            for i in &b.instructions {
                if let Some(v) = &i.result {
                    let _ = write!(o, "    %{}: {} = ", v.id.0, v.ty);
                } else {
                    let _ = write!(o, "    ");
                }
                match &i.kind {
                    InstructionKind::Constant(Constant::Integer(v)) => {
                        let _ = writeln!(o, "const {v}");
                    }
                    InstructionKind::Constant(Constant::Float(v)) => {
                        let _ = writeln!(o, "const {v}");
                    }
                    InstructionKind::Constant(Constant::String(v)) => {
                        let _ = writeln!(o, "const {v:?}");
                    }
                    InstructionKind::Constant(Constant::Bool(v)) => {
                        let _ = writeln!(o, "const {v}");
                    }
                    InstructionKind::Binary { op, left, right } => {
                        let _ = writeln!(o, "{} %{}, %{}", op, left.id.0, right.id.0);
                    }
                    InstructionKind::Call { function, args } => {
                        let a = args
                            .iter()
                            .map(|x| format!("%{}", x.id.0))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let _ = writeln!(o, "call @{function}({a})");
                    }
                    InstructionKind::Phi { incoming } => {
                        let a = incoming
                            .iter()
                            .map(|(b, v)| format!("[ %{}, b{} ]", v.id.0, b.0))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let _ = writeln!(o, "phi {a}");
                    }
                }
            }
            match &b.terminator {
                Some(Terminator::Return(Some(v))) => {
                    let _ = writeln!(o, "    ret %{}", v.id.0);
                }
                Some(Terminator::Return(None)) => {
                    let _ = writeln!(o, "    ret void");
                }
                Some(Terminator::Branch(to)) => {
                    let _ = writeln!(o, "    br b{}", to.0);
                }
                Some(Terminator::ConditionalBranch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    let _ = writeln!(
                        o,
                        "    condbr %{}, b{}, b{}",
                        condition.id.0, then_block.0, else_block.0
                    );
                }
                Some(Terminator::Unreachable) => {
                    let _ = writeln!(o, "    unreachable");
                }
                None => {
                    let _ = writeln!(o, "    <unterminated>");
                }
            }
        }
        let _ = writeln!(o, "}}");
    }
    o
}
