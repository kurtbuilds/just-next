use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Export, Settings};

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
        let dotenv_path = if let Some(p) = custom_path {
            self.working_dir.join(p)
        } else {
            // Search order: .env.local, .env. Walk up the directory tree,
            // stopping at the first directory that contains a match.
            let candidates = [".env.local", ".env"];
            let mut dir = Some(self.working_dir.as_path());
            let found = loop {
                let Some(current) = dir else { break None };
                if let Some(p) = candidates
                    .iter()
                    .map(|p| current.join(p))
                    .find(|p| p.exists())
                {
                    break Some(p);
                }
                dir = current.parent();
            };
            match found {
                Some(p) => p,
                None => return,
            }
        };

        if !dotenv_path.exists() {
            return;
        }

        if let Ok(content) = std::fs::read_to_string(&dotenv_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    // Remove quotes if present
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                        .unwrap_or(value);

                    // Don't override existing env vars
                    if !self.vars.contains_key(key) {
                        self.vars.insert(key.to_string(), value.to_string());
                    }
                }
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
}
