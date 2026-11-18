use super::Type;
use crate::lexer::token::Span;

/// A parameter constraint in a backend-neutral function signature.
/// `AnyValue` is reserved for polymorphic built-ins such as `println`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParameterType {
    Exact(Type),
    AnyValue,
}
impl ParameterType {
    pub fn accepts(self, ty: Type) -> bool {
        match self {
            Self::Exact(expected) => expected == ty,
            Self::AnyValue => ty != Type::Void,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    pub params: Vec<ParameterType>,
    pub return_type: Type,
}
impl Signature {
    pub fn exact(params: impl IntoIterator<Item = Type>, return_type: Type) -> Self {
        Self {
            params: params.into_iter().map(ParameterType::Exact).collect(),
            return_type,
        }
    }
    pub fn any_value(return_type: Type) -> Self {
        Self {
            params: vec![ParameterType::AnyValue],
            return_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFunction {
    pub name: String,
    pub signature: Signature,
    pub span: Span,
}
impl ExternalFunction {
    pub fn new(name: impl Into<String>, signature: Signature, span: Span) -> Self {
        Self {
            name: name.into(),
            signature,
            span,
        }
    }
}
