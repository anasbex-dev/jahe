//! The verified-module boundary consumed by backends.
use super::{Diagnostic, Module, Type};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct VerifiedModule<'m> {
    module: &'m Module,
}
impl<'m> VerifiedModule<'m> {
    pub fn module(&self) -> &'m Module {
        self.module
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Verifier;
impl Verifier {
    pub fn verify<'m>(&self, module: &'m Module) -> Result<VerifiedModule<'m>, Vec<Diagnostic>> {
        super::validate::validate(module)?;
        Ok(VerifiedModule { module })
    }
    /// Compatibility hook for tools that supply temporary external signatures.
    /// Runtime backends should prefer signatures declared directly in the IR module.
    pub fn verify_with_externals<'m>(
        &self,
        module: &'m Module,
        externals: &HashMap<String, (Vec<Type>, Type)>,
    ) -> Result<VerifiedModule<'m>, Vec<Diagnostic>> {
        super::validate::validate_with_externals(module, externals)?;
        Ok(VerifiedModule { module })
    }
}
pub fn verify(module: &Module) -> Result<VerifiedModule<'_>, Vec<Diagnostic>> {
    Verifier.verify(module)
}
