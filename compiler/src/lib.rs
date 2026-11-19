pub mod backend;
pub mod diagnostics;
pub mod frontend;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod target;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
