use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::v2::ast::{Export, Settings};

/// Find the `.env` file that applies to `dir`.
///
/// Search order is `.env.local`, then `.env`, walking up the directory tree and
/// stopping at the first directory that holds either.
pub fn find_dotenv(dir: &Path) -> Option<PathBuf> {
    let candidates = [".env.local", ".env"];
    let mut dir = Some(dir);
    loop {
        let current = dir?;
        if let Some(path) = candidates
            .iter()
            .map(|p| current.join(p))
            .find(|p| p.exists())
        {
            return Some(path);
        }
        dir = current.parent();
    }
}

/// Read a `.env` file into key/value pairs, in file order. A file that cannot
/// be read yields nothing.
pub fn read_dotenv(path: &Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content.lines().filter_map(parse_dotenv_line).collect()
}

/// Parse one line of a `.env` file into a key and value.
///
/// A line this does not understand is skipped rather than guessed at.
fn parse_dotenv_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (key, value) = line.split_once('=')?;

    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    Some((key.to_string(), parse_dotenv_value(value.trim())))
}

/// Unwrap a `.env` value: strip a matching pair of quotes, and drop any comment
/// that follows the value.
fn parse_dotenv_value(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            // Take everything up to the closing quote; what follows it is a
            // comment, not part of the value. An unterminated quote is left
            // alone — better a value with a stray quote in it than one
            // silently truncated.
            if let Some(end) = rest.find(quote) {
                return rest[..end].to_string();
            }
            return value.to_string();
        }
    }

    // An unquoted value ends at a ` #` comment. `#` with no space before it is
    // part of the value, so a URL fragment or a password survives.
    match value.split_once(" #").or_else(|| value.split_once("\t#")) {
        Some((value, _)) => value.trim_end().to_string(),
        None => value.to_string(),
    }
}

/// Environment for recipe execution
pub struct Environment {
    vars: HashMap<String, String>,
    working_dir: PathBuf,
}

impl Environment {
    /// Create a new environment from the current process environment
    pub fn new(working_dir: PathBuf) -> Self {
        // Make the directory absolute without resolving symlinks, so $PWD reads
        // the way the user would write it
        let working_dir = if working_dir.is_absolute() {
            working_dir
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(&working_dir))
                .unwrap_or(working_dir)
        };

        let mut vars: HashMap<String, String> = std::env::vars().collect();
        // Recipes run in the justfile's directory, which may differ from the
        // directory we were invoked from, so $PWD must follow it.
        vars.insert("PWD".to_string(), working_dir.display().to_string());
        vars.remove("OLDPWD");
        Self { vars, working_dir }
    }

    /// Set up the environment based on justfile settings
    pub fn setup(&mut self, settings: &Settings, exports: &[Export]) {
        // 1. Load dotenv if enabled
        if settings.dotenv {
            self.load_dotenv(settings.dotenv_path.as_deref());
        }

        // 2. Set up PATH with node_modules/.bin
        self.setup_node_path();

        // 3. Set up PATH with cargo target/debug
        self.setup_cargo_path();

        // 4. Activate virtual environment
        self.setup_venv(settings.venv.as_deref());

        // 4. Process exports from justfile
        for export in exports {
            let value = self.expand_value(&export.value);
            self.vars.insert(export.name.clone(), value);
        }
    }

    /// Load .env file(s)
    fn load_dotenv(&mut self, custom_path: Option<&str>) {
        let dotenv_path = match custom_path {
            Some(p) => self.working_dir.join(p),
            None => match find_dotenv(&self.working_dir) {
                Some(p) => p,
                None => return,
            },
        };

        for (key, value) in read_dotenv(&dotenv_path) {
            // Don't override existing env vars
            if !self.vars.contains_key(&key) {
                self.vars.insert(key, value);
            }
        }
    }

    /// Add node_modules/.bin to PATH
    fn setup_node_path(&mut self) {
        let node_bin = self.working_dir.join("node_modules/.bin");
        if node_bin.exists() {
            self.prepend_path(&node_bin);
        }
    }

    /// Add cargo target/debug to PATH
    fn setup_cargo_path(&mut self) {
        // Check CARGO_TARGET_DIR first, fall back to target/debug
        let cargo_bin = if let Some(target_dir) = self.vars.get("CARGO_TARGET_DIR") {
            PathBuf::from(target_dir).join("debug")
        } else {
            self.working_dir.join("target/debug")
        };

        if cargo_bin.exists() {
            self.append_path(&cargo_bin);
        }
    }

    /// Set up Python virtual environment
    fn setup_venv(&mut self, custom_path: Option<&str>) {
        let venv_path = if let Some(p) = custom_path {
            let path = self.working_dir.join(p);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        } else {
            // Auto-detect venv
            let candidates = [".venv", "venv", ".uv/venv"];
            candidates
                .iter()
                .map(|p| self.working_dir.join(p))
                .find(|p| p.join("bin/activate").exists() || p.join("Scripts/activate").exists())
        };

        if let Some(venv) = venv_path {
            // Add venv/bin to PATH
            let bin_dir = if cfg!(windows) {
                venv.join("Scripts")
            } else {
                venv.join("bin")
            };

            if bin_dir.exists() {
                self.prepend_path(&bin_dir);
            }

            // Set VIRTUAL_ENV
            self.vars
                .insert("VIRTUAL_ENV".to_string(), venv.display().to_string());

            // Unset PYTHONHOME if set
            self.vars.remove("PYTHONHOME");
        }
    }

    /// Prepend a directory to PATH
    fn prepend_path(&mut self, dir: &Path) {
        let current_path = self.vars.get("PATH").cloned().unwrap_or_default();
        let new_path = format!("{}:{}", dir.display(), current_path);
        self.vars.insert("PATH".to_string(), new_path);
    }

    /// Append a directory to PATH
    fn append_path(&mut self, dir: &Path) {
        let current_path = self.vars.get("PATH").cloned().unwrap_or_default();
        let new_path = format!("{}:{}", current_path, dir.display());
        self.vars.insert("PATH".to_string(), new_path);
    }

    /// Expand environment variables in a value
    pub fn expand_value(&self, value: &str) -> String {
        let mut result = String::new();
        let mut chars = value.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' {
                // Variable reference
                let var_name = if chars.peek() == Some(&'{') {
                    chars.next(); // consume {
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        name.push(chars.next().unwrap());
                    }
                    name
                } else {
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    name
                };

                if let Some(val) = self.vars.get(&var_name) {
                    result.push_str(val);
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Set a variable
    #[allow(dead_code)]
    pub fn set(&mut self, key: String, value: String) {
        self.vars.insert(key, value);
    }

    /// Get a variable
    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    /// Get all variables as a reference
    pub fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    /// Get working directory
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_value() {
        let mut env = Environment::new(PathBuf::from("/tmp"));
        env.set("FOO".to_string(), "bar".to_string());
        env.set("PATH".to_string(), "/usr/bin".to_string());

        assert_eq!(env.expand_value("$FOO"), "bar");
        assert_eq!(env.expand_value("${FOO}"), "bar");
        assert_eq!(env.expand_value("prefix:$PATH"), "prefix:/usr/bin");
        assert_eq!(env.expand_value("$FOO/$PATH"), "bar//usr/bin");
    }

    fn parse(line: &str) -> Option<(String, String)> {
        parse_dotenv_line(line)
    }

    fn pair(key: &str, value: &str) -> Option<(String, String)> {
        Some((key.to_string(), value.to_string()))
    }

    #[test]
    fn test_parse_dotenv_line() {
        assert_eq!(parse("URL=http://x.io"), pair("URL", "http://x.io"));
        assert_eq!(parse("URL = http://x.io"), pair("URL", "http://x.io"));
        assert_eq!(parse("  URL=http://x.io  "), pair("URL", "http://x.io"));
        assert_eq!(parse("URL=\"http://x.io\""), pair("URL", "http://x.io"));
        assert_eq!(parse("URL='http://x.io'"), pair("URL", "http://x.io"));

        // Trailing comments.
        assert_eq!(parse("URL=\"x\" # comment"), pair("URL", "x"));
        assert_eq!(parse("URL=x # comment"), pair("URL", "x"));
        assert_eq!(parse("URL=http://x.io/#frag"), pair("URL", "http://x.io/#frag"));

        // Values that keep what looks like syntax.
        assert_eq!(parse("URL=a=b"), pair("URL", "a=b"));
        assert_eq!(parse("URL=\"a b\""), pair("URL", "a b"));
        assert_eq!(parse("URL=\"unterminated"), pair("URL", "\"unterminated"));
        assert_eq!(parse("EMPTY="), pair("EMPTY", ""));

        // Not assignments.
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
        assert_eq!(parse("# URL=x"), None);
        assert_eq!(parse("URL"), None);
        assert_eq!(parse("=x"), None);
    }
}
