use crate::{
    backend::interpreter::error::RuntimeError,
    ir::{Diagnostic, VerifiedModule},
};
use std::fmt;

pub trait Backend {
    type Output;
    type Error;
    fn execute(&self, module: &VerifiedModule<'_>) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendErrorKind {
    Verification,
    MissingEntry,
    InvalidEntrySignature,
    ArgumentMismatch,
    ExternalFunctionUnresolved,
    ExternalSignatureMismatch,
    Runtime,
    NativeCodegen,
    Linker,
    RuntimeAbiMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
    pub runtime: Option<RuntimeError>,
}
impl BackendError {
    pub fn verification(errors: Vec<Diagnostic>) -> Self {
        Self {
            kind: BackendErrorKind::Verification,
            message: "IR verification failed".into(),
            diagnostics: errors,
            runtime: None,
        }
    }
    pub fn new(kind: BackendErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            diagnostics: Vec::new(),
            runtime: None,
        }
    }
    pub fn runtime(error: RuntimeError) -> Self {
        Self {
            kind: BackendErrorKind::Runtime,
            message: error.message.clone(),
            diagnostics: Vec::new(),
            runtime: Some(error),
        }
    }
}
impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        for d in &self.diagnostics {
            write!(
                f,
                "\n[{}] {} at {}:{}",
                d.code, d.message, d.span.line, d.span.column
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for BackendError {}
