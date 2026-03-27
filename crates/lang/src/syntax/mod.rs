#![allow(missing_docs)]

mod features;
mod lexer;
mod parser;
mod token;

pub use parser::parse_program;
