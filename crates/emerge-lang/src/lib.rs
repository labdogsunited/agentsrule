//! Parser and formatter for the Emergent `.em` language (`language.md`).
//!
//! `parse` turns source text into a syntax tree that keeps comments and blank lines;
//! `format` prints the one canonical text of a tree (`emerge fmt`); `check::check` runs
//! the static checks written so far (chemistry). `emerge-ref` compiles a checked tree
//! into a runnable world. Type checking comes next.

pub mod ast;
pub mod check;
pub mod contract;
pub mod expand;
pub mod fmt;
pub mod graph;
pub mod lexer;
pub mod look;
pub mod order;
pub mod schedule;
pub mod unroll;
pub mod parser;
pub mod patch;
pub mod resolve;
pub mod view;

pub use ast::File;
pub use expand::expand;
pub use resolve::resolve;

/// What `check` and the runner's `build` work on: lineages expanded (`expand`), then
/// the loops of rules unrolled (`unroll`), then constants resolved (`resolve`).
pub fn prepare(file: &File) -> Result<File, check::Problem> {
    resolve(&unroll::unroll(&expand(file)?)?)
}

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
