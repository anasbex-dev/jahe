//! SSA invariant helpers. Phi inputs are associated with predecessor block IDs.
use super::{BlockId, Value};
pub fn phi_inputs_match_predecessors(
    incoming: &[(BlockId, Value)],
    predecessors: &[BlockId],
) -> bool {
    let got: std::collections::HashSet<_> = incoming.iter().map(|(b, _)| *b).collect();
    let want: std::collections::HashSet<_> = predecessors.iter().copied().collect();
    incoming.len() == got.len()
        && predecessors.len() == want.len()
        && got == want
        && incoming
            .iter()
            .all(|(_, v)| v.ty == incoming.first().map(|(_, x)| x.ty).unwrap_or(v.ty))
}
