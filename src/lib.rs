pub mod ast;
pub mod detection;
pub mod environment;
pub mod error;
pub mod executor;
pub mod parser;

pub use ast::Justfile;
pub use error::{Error, Result};
pub use parser::parse;
