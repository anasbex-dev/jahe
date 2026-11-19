use super::{
    error::{RuntimeError, RuntimeErrorKind},
    value::RuntimeValue,
};
use crate::{
    ir::{ParameterType, Signature, Type},
    lexer::token::Span,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct HostContext {
    pub output: String,
}
pub trait HostFunction: Send + Sync {
    fn signature(&self) -> Signature;
    fn call(
        &self,
        args: &[RuntimeValue],
        context: &mut HostContext,
        span: Span,
    ) -> Result<Option<RuntimeValue>, RuntimeError>;
}
#[derive(Default, Clone)]
pub struct BuiltinRegistry {
    functions: HashMap<String, Arc<dyn HostFunction>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationErrorKind {
    EmptyName,
    DuplicateName,
    InvalidSignature,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationError {
    pub kind: RegistrationErrorKind,
    pub message: String,
}
impl BuiltinRegistry {
    pub fn with_standard() -> Self {
        Self {
            functions: HashMap::from([(
                "println".into(),
                Arc::new(Println) as Arc<dyn HostFunction>,
            )]),
        }
    }
    pub fn register(
        &mut self,
        name: impl Into<String>,
        function: impl HostFunction + 'static,
    ) -> Result<(), RegistrationError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(RegistrationError {
                kind: RegistrationErrorKind::EmptyName,
                message: "host function name cannot be empty".into(),
            });
        }
        if self.functions.contains_key(&name) {
            return Err(RegistrationError {
                kind: RegistrationErrorKind::DuplicateName,
                message: format!("host function `{name}` is already registered"),
            });
        }
        if function
            .signature()
            .params
            .iter()
            .any(|p| matches!(p, ParameterType::Exact(Type::Void | Type::Never)))
        {
            return Err(RegistrationError {
                kind: RegistrationErrorKind::InvalidSignature,
                message: format!("host function `{name}` cannot accept void parameters"),
            });
        }
        self.functions.insert(name, Arc::new(function));
        Ok(())
    }
    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
    pub fn signature(&self, name: &str) -> Option<Signature> {
        self.functions.get(name).map(|f| f.signature())
    }
    pub fn signatures(&self) -> HashMap<String, Signature> {
        self.functions
            .iter()
            .map(|(n, f)| (n.clone(), f.signature()))
            .collect()
    }
    pub(crate) fn get(&self, name: &str) -> Option<Arc<dyn HostFunction>> {
        self.functions.get(name).cloned()
    }
}
struct Println;
impl HostFunction for Println {
    fn signature(&self) -> Signature {
        Signature {
            params: vec![ParameterType::AnyValue],
            return_type: Type::Void,
        }
    }
    fn call(
        &self,
        args: &[RuntimeValue],
        context: &mut HostContext,
        span: Span,
    ) -> Result<Option<RuntimeValue>, RuntimeError> {
        if args.len() != 1 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArgumentCount,
                "println expects one argument",
                Some(span),
            ));
        }
        context.output.push_str(&args[0].to_string());
        context.output.push('\n');
        Ok(None)
    }
}
