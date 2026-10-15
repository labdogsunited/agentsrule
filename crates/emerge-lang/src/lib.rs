//! Parser and formatter for the Emergent `.em` language (`language.md`).
//!
//! `parse` turns source text into a syntax tree that keeps comments and blank lines;
//! `format` prints the one canonical text of a tree (`emerge fmt`); `check::check` runs
//! the static checks written so far (chemistry). `emerge-ref` compiles a checked tree
//! into a runnable world. Type checking comes next.

pub mod ast;
pub mod check;
pub mod fmt;
pub mod lexer;
pub mod parser;

pub use ast::File;

/// A diagnostic with its code (`language.md` §14) and position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub code: &'static str,
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}:{}: {}", self.code, self.line, self.col, self.message)
    }
}

pub fn parse(src: &str) -> Result<File, Diag> {
    parser::Parser::new(lexer::lex(src)?).file()
}

pub fn format(file: &File) -> String {
    fmt::format(file)
}
