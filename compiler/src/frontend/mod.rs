use crate::diagnostics::Diagnostic;
use crate::semantic::SemanticDiagnostic;
use crate::{
    lexer::{token::Token, Lexer},
    parser::{self, ast::Module},
};

pub fn parse(source: &str) -> Result<Module, Diagnostic> {
    parser::parse(source)
}
pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).tokenize()
}

pub fn analyze(source: &str) -> Result<Vec<SemanticDiagnostic>, Diagnostic> {
    let module = parse(source)?;
    Ok(crate::semantic::analyze(&module))
}

pub fn lower_to_ir(source: &str) -> Result<crate::ir::Module, Vec<crate::ir::Diagnostic>> {
    let module = parse(source).map_err(|error| {
        vec![crate::ir::Diagnostic {
            code: "E0001",
            message: error.message,
            span: error.span,
        }]
    })?;
    crate::ir::lower::lower(&module)
}
