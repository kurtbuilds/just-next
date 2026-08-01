use std::process::Command;

/// Style of justfile detected
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustfileStyle {
    /// Old just syntax detected - should exec original just
    Legacy,
    /// Next style - use our implementation
    Next,
}

/// Patterns that indicate legacy just syntax
const LEGACY_PATTERNS: &[&str] = &[
    // Assignment syntax with :=
    ":=",
    // Built-in functions
    "env_var(",
    "env_var_or_default(",
    "env(",
    "invocation_directory(",
    "justfile(",
    "justfile_directory(",
    "just_executable(",
    "source_directory(",
    "source_file(",
    // Conditionals
    "if ",
    "else ",
    // Error function
    "error(",
    // Old setting names
    "dotenv-load",
    "set fallback",
    "set ignore-comments",
    "set allow-duplicate-recipes",
    "set tempdir",
    "set windows-powershell",
    "set windows-shell",
    // Import
    "import ",
    "mod ",
];

/// Detect the style of a justfile
pub fn detect_style(content: &str) -> JustfileStyle {
    // Explicit next mode
    if content.contains("set next") {
        return JustfileStyle::Next;
    }

    // Check for legacy patterns
    for &pattern in LEGACY_PATTERNS {
        if content.contains(pattern) {
            return JustfileStyle::Legacy;
        }
    }

    // Default to next mode
    JustfileStyle::Next
}

/// Execute the original just binary, replacing this process
///
/// `args` overrides the arguments forwarded to `just`. When `None`, our own
/// arguments are passed through verbatim.
pub fn exec_legacy_just(just_path: Option<&str>, args: Option<Vec<String>>) -> ! {
    let args: Vec<String> = args.unwrap_or_else(|| std::env::args().skip(1).collect());

    let just_binary = if let Some(path) = just_path {
        path.to_string()
    } else {
        find_just_binary()
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        let err = Command::new(&just_binary).args(&args).exec();

        eprintln!("just-next: failed to exec '{}': {}", just_binary, err);
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, spawn and wait
        match Command::new(&just_binary).args(&args).status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("just-next: failed to run '{}': {}", just_binary, e);
                std::process::exit(1);
            }
        }
    }
}

/// Find the original just binary in PATH
fn find_just_binary() -> String {
    // Get our own executable path to avoid finding ourselves
    let our_path = std::env::current_exe().ok();

    match which::which("just") {
        Ok(path) => {
            // Make sure it's not us
            if let Some(ref our) = our_path {
                if path == *our {
                    // Try to find another just
                    if let Ok(all) = which::which_all("just") {
                        for p in all {
                            if p != *our {
                                return p.to_string_lossy().to_string();
                            }
                        }
                    }
                }
            }
            path.to_string_lossy().to_string()
        }
        Err(_) => {
            eprintln!("just-next: legacy just binary not found in PATH");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_next_explicit() {
        let content = r#"
set next

build:
    cargo build
"#;
        assert_eq!(detect_style(content), JustfileStyle::Next);
    }

    #[test]
    fn test_detect_legacy_assignment() {
        let content = r#"
foo := "bar"

build:
    echo {{foo}}
"#;
        assert_eq!(detect_style(content), JustfileStyle::Legacy);
    }

    #[test]
    fn test_detect_legacy_function() {
        let content = r#"
build:
    echo {{env_var("HOME")}}
"#;
        assert_eq!(detect_style(content), JustfileStyle::Legacy);
    }

    #[test]
    fn test_detect_next_default() {
        let content = r#"
export PATH="node_modules/.bin:$PATH"

build:
    cargo build
"#;
        assert_eq!(detect_style(content), JustfileStyle::Next);
    }
}
