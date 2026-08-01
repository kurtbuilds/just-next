//! The next-generation justfile engine.
//!
//! This is just-next's own parser and executor, implementing the simplified
//! syntax described in the README: shell-style `export`, recipe-body variable
//! assignments that persist across lines, and automatic argument quoting.
//!
//! Justfiles reach this engine only when [`crate::dispatch`] identifies them as
//! V2. Everything else runs through [`crate::v1`], which is upstream `just`.

pub mod ast;
pub mod cli;
pub mod environment;
pub mod error;
pub mod executor;
pub mod parser;

pub use ast::Justfile;
pub use error::{Error, Result};
pub use parser::parse;
