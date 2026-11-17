use crate::lexer::token::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

pub fn render(filename: &str, source: &str, diagnostic: &Diagnostic) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines
        .get(diagnostic.span.line.saturating_sub(1))
        .copied()
        .unwrap_or("");
    let width = diagnostic.span.line.to_string().len();
    format!(
        "error: {}\n --> {}:{}:{}\n  |\n{:>width$} | {}\n  | {:>column$}^",
        diagnostic.message,
        filename,
        diagnostic.span.line,
        diagnostic.span.column,
        diagnostic.span.line,
        line,
        "",
        column = diagnostic.span.column.saturating_sub(1),
        width = width
    )
}

pub fn render_semantic(
    filename: &str,
    source: &str,
    diagnostic: &crate::semantic::SemanticDiagnostic,
) -> String {
    let base = render(
        filename,
        source,
        &Diagnostic::new(
            format!("[{}] {}", diagnostic.code, diagnostic.message),
            diagnostic.span,
        ),
    );
    match (&diagnostic.expected, &diagnostic.actual) {
        (Some(expected), Some(actual)) => {
            format!("{base}\n  expected: {expected}\n  actual:   {actual}")
        }
        _ => base,
    }
}

pub fn render_ir(filename: &str, source: &str, diagnostic: &crate::ir::Diagnostic) -> String {
    render(
        filename,
        source,
        &Diagnostic::new(
            format!("[{}] {}", diagnostic.code, diagnostic.message),
            diagnostic.span,
        ),
    )
}
