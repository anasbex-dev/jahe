use crate::diagnostics::Diagnostic;
use crate::semantic::SemanticDiagnostic;
use crate::{
    lexer::{token::Token, Lexer},
    parser::{self, ast::Module, SyntaxMode},
};

pub fn parse(source: &str) -> Result<Module, Diagnostic> {
    parser::parse(source)
}
pub fn parse_with_mode(source: &str, mode: SyntaxMode) -> Result<Module, Diagnostic> {
    parser::parse_with_mode(source, mode)
}
pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).tokenize()
}

pub fn analyze(source: &str) -> Result<Vec<SemanticDiagnostic>, Diagnostic> {
    let module = parse(source)?;
    Ok(crate::semantic::analyze(&module))
}
pub fn analyze_with_mode(
    source: &str,
    mode: SyntaxMode,
) -> Result<Vec<SemanticDiagnostic>, Diagnostic> {
    let module = parse_with_mode(source, mode)?;
    Ok(crate::semantic::analyze(&module))
}

pub fn lower_to_ir(source: &str) -> Result<crate::ir::Module, Vec<crate::ir::Diagnostic>> {
    lower_to_ir_with_mode(source, SyntaxMode::Canonical)
}
pub fn lower_to_ir_with_mode(
    source: &str,
    mode: SyntaxMode,
) -> Result<crate::ir::Module, Vec<crate::ir::Diagnostic>> {
    let module = parse_with_mode(source, mode).map_err(|error| {
        vec![crate::ir::Diagnostic {
            code: "E0001",
            message: error.message,
            span: error.span,
        }]
    })?;
    crate::ir::lower::lower(&module)
}
