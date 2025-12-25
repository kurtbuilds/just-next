use miette::Diagnostic;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error("Parse error at line {line}: {message}")]
    #[diagnostic(code(just::parse_error))]
    Parse { line: usize, message: String },

    #[error("Recipe '{0}' not found")]
    #[diagnostic(code(just::recipe_not_found))]
    RecipeNotFound(String),

    #[error("Missing required argument '{0}'")]
    #[diagnostic(code(just::missing_argument))]
    MissingArgument(String),

    #[error("Recipe '{0}' failed with exit code {1}")]
    #[diagnostic(code(just::recipe_failed))]
    RecipeFailed(String, i32),

    #[error("Failed to execute command: {0}")]
    #[diagnostic(code(just::exec_failed))]
    ExecFailed(#[from] std::io::Error),

    #[error("No justfile found")]
    #[diagnostic(code(just::no_justfile))]
    NoJustfile,

    #[error("Failed to read {path}: {source}")]
    #[diagnostic(code(just::read_failed))]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Circular dependency detected: {0}")]
    #[diagnostic(code(just::circular_dependency))]
    CircularDependency(String),
}

pub type Result<T> = std::result::Result<T, Error>;
