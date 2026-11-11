//! ICE snippets: a small language for ICE graphs, OSL's syntax and types with
//! ICE's model of geometry (`docs/ice-snippets.md`). A snippet is not run by
//! a second engine: it is parsed (`parse`), then lowered (`lower`) into nodes
//! of the engine's own graph, which compile and run as any ICE graph's do.

mod lex;
mod lower;
mod parse;

pub use lex::Pos;
pub use lower::{default_value, lower, pinned, ports, Lowered, Port, LOOP_LIMIT};
pub use parse::{parse, Expr, ExprKind, Param, Snippet, Type};

/// What is wrong with a snippet, and where in its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
}

impl Error {
    pub fn at(pos: Pos, message: impl Into<String>) -> Error {
        Error {
            pos,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "line {}, col {}: {}", self.pos.line, self.pos.col, self.message)
    }
}

#[cfg(test)]
mod tests;
