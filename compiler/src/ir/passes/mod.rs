//! Minimal pass interface. Passes mutate JAHE IR and report structured diagnostics.
use super::{Diagnostic, Module};
pub trait Pass {
    fn name(&self) -> &'static str;
    fn run(&mut self, module: &mut Module) -> Result<(), Vec<Diagnostic>>;
}
pub struct ValidatePass;
impl Pass for ValidatePass {
    fn name(&self) -> &'static str {
        "validate"
    }
    fn run(&mut self, module: &mut Module) -> Result<(), Vec<Diagnostic>> {
        super::validate::validate(module)
    }
}
