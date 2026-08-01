use std::collections::HashMap;

/// Represents a complete justfile
#[derive(Debug, Clone, Default)]
pub struct Justfile {
    /// Settings like `set dotenv`, `set export`
    pub settings: Settings,
    /// Top-level exports
    pub exports: Vec<Export>,
    /// Recipe definitions
    pub recipes: Vec<Recipe>,
    /// Aliases: alias name -> recipe name
    pub aliases: HashMap<String, String>,
}

/// Parsed settings from `set` statements
#[derive(Debug, Clone)]
pub struct Settings {
    /// Force next mode - disables backward compat detection
    pub next: bool,
    /// Load .env files (default: true in next mode)
    pub dotenv: bool,
    /// Custom dotenv path
    pub dotenv_path: Option<String>,
    /// Export all variables (default: true in next mode)
    pub export: bool,
    /// Enable positional arguments (default: true in next mode)
    pub positional_arguments: bool,
    /// Custom venv path
    pub venv: Option<String>,
    /// Shell to use
    pub shell: Option<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            next: false,
            dotenv: true,
            dotenv_path: None,
            export: true,
            positional_arguments: true,
            venv: None,
            shell: None,
        }
    }
}

/// Export statement: `export PATH="..."`
#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub value: String,
}

/// A recipe definition
#[derive(Debug, Clone)]
pub struct Recipe {
    pub name: String,
    pub doc: Option<String>,
    pub parameters: Vec<Parameter>,
    pub dependencies: Vec<Dependency>,
    pub body: Vec<Line>,
    /// Whether recipe runs in quiet mode (@)
    pub quiet: bool,
    /// Shebang if present (#!/bin/bash)
    pub shebang: Option<String>,
}

/// Recipe parameter
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub kind: ParameterKind,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterKind {
    /// Regular parameter
    Normal,
    /// Variadic (*args)
    Variadic,
    /// Plus variadic (+args) - requires at least one
    PlusVariadic,
}

/// Recipe dependency
#[derive(Debug, Clone)]
pub struct Dependency {
    pub recipe: String,
    pub arguments: Vec<String>,
}

/// A line in a recipe body
#[derive(Debug, Clone)]
pub enum Line {
    /// Regular command
    Command(Command),
    /// Variable assignment within recipe: FOO=bar
    Assignment { name: String, value: String },
    /// Export within recipe: export FOO=bar
    Export { name: String, value: String },
}

/// A command line
#[derive(Debug, Clone)]
pub struct Command {
    /// The raw command text
    pub text: String,
    /// Quiet (@) prefix
    pub quiet: bool,
    /// Error suppression (-) prefix
    pub ignore_errors: bool,
}
