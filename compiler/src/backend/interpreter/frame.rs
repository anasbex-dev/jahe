use super::value::RuntimeValue;
use crate::ir::{BlockId, ValueId};
use std::collections::HashMap;
pub struct Frame {
    pub values: HashMap<ValueId, RuntimeValue>,
    pub predecessor: Option<BlockId>,
    pub block: BlockId,
}
