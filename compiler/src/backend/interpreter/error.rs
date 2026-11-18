use crate::lexer::token::Span;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    MissingFunction,
    ArgumentCount,
    ArgumentType,
    MissingValue,
    InvalidValue,
    DivisionByZero,
    IntegerOverflow,
    StepLimitExceeded,
    CallDepthExceeded,
    InvalidControlFlow,
    InvalidEntryReturn,
    HostFunction,
    UnresolvedExternal,
    ExternalArgumentMismatch,
    ExternalReturnMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub message: String,
    pub span: Option<Span>,
}
impl RuntimeError {
    pub fn new(kind: RuntimeErrorKind, message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
        }
    }
}
