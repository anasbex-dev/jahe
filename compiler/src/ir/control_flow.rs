//! Helpers for reasoning about CFG edges. All targets are function-local `BlockId`s.
use super::{BlockId, Function, Terminator};
use std::collections::HashMap;
pub fn predecessors(function: &Function) -> HashMap<BlockId, Vec<BlockId>> {
    let mut p: HashMap<BlockId, Vec<BlockId>> =
        function.blocks.iter().map(|b| (b.id, Vec::new())).collect();
    for b in &function.blocks {
        let targets = match &b.terminator {
            Some(Terminator::Branch(t)) => vec![*t],
            Some(Terminator::ConditionalBranch {
                then_block,
                else_block,
                ..
            }) => vec![*then_block, *else_block],
            _ => vec![],
        };
        for t in targets {
            if let Some(v) = p.get_mut(&t) {
                if !v.contains(&b.id) {
                    v.push(b.id);
                }
            }
        }
    }
    p
}
