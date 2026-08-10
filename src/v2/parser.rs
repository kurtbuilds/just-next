use crate::v2::ast::*;
use crate::v2::error::{Error, Result};

pub fn parse(input: &str) -> Result<Justfile> {
    let mut parser = Parser::new(input);
    parser.parse()
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lines: input.lines().collect(),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Result<Justfile> {
        let mut justfile = Justfile::default();
        let mut pending_doc: Option<String> = None;

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos];
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                pending_doc = None;
                self.pos += 1;
                continue;
            }

            // Comments
            if trimmed.starts_with('#') {
                // Check for doc comment
                if trimmed.starts_with("# ") || trimmed == "#" {
                    pending_doc = Some(trimmed.trim_start_matches('#').trim().to_string());
                }
                self.pos += 1;
                continue;
            }

            // Set statement
            if trimmed.starts_with("set ") {
                self.parse_setting(&mut justfile.settings)?;
                pending_doc = None;
                continue;
            }

            // Export statement
            if trimmed.starts_with("export ") {
                if let Some(export) = self.parse_export()? {
                    justfile.exports.push(export);
                }
                pending_doc = None;
                continue;
            }

            // Alias
            if trimmed.starts_with("alias ") {
                let (name, target) = self.parse_alias()?;
                justfile.aliases.insert(name, target);
                pending_doc = None;
                continue;
            }

            // Recipe
            if self.is_recipe_header(trimmed) {
                let mut recipe = self.parse_recipe()?;
                recipe.doc = pending_doc.take();
                justfile.recipes.push(recipe);
                continue;
            }

            // Unknown line
            return Err(Error::Parse {
                line: self.pos + 1,
                message: format!("unexpected line: {}", trimmed),
            });
        }

        Ok(justfile)
    }

    fn parse_setting(&mut self, settings: &mut Settings) -> Result<()> {
        let line = self.lines[self.pos].trim();
        let rest = line.strip_prefix("set ").unwrap().trim();
        self.pos += 1;

        // Parse: name or name = value
        if let Some((name, value)) = rest.split_once('=') {
            let name = name.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            match name {
                "dotenv" => {
                    settings.dotenv = true;
                    if !value.is_empty() {
                        settings.dotenv_path = Some(value.to_string());
                    }
                }
                "venv" => settings.venv = Some(value.to_string()),
                "shell" => {
                    // Parse array: ["bash", "-c"]
                    settings.shell = Some(parse_string_array(value));
                }
                _ => {} // Ignore unknown settings
            }
        } else {
            match rest {
                "next" => settings.next = true,
                "dotenv" => settings.dotenv = true,
                "export" => settings.export = true,
                "positional-arguments" => settings.positional_arguments = true,
                _ => {} // Ignore unknown settings
            }
        }

        Ok(())
    }

    fn parse_export(&mut self) -> Result<Option<Export>> {
        let line = self.lines[self.pos].trim();
        let rest = line.strip_prefix("export ").unwrap().trim();
        self.pos += 1;

        // Parse: NAME="value" or NAME=value
        if let Some((name, value)) = rest.split_once('=') {
            let name = name.trim();
            let value = parse_quoted_value(value.trim());

            return Ok(Some(Export {
                name: name.to_string(),
                value,
            }));
        }

        Ok(None)
    }

    fn parse_alias(&mut self) -> Result<(String, String)> {
        let line = self.lines[self.pos].trim();
        let rest = line.strip_prefix("alias ").unwrap().trim();
        self.pos += 1;

        // Parse: name := target
        if let Some((name, target)) = rest.split_once(":=") {
            return Ok((name.trim().to_string(), target.trim().to_string()));
        }

        // Also support: name = target (next style)
        if let Some((name, target)) = rest.split_once('=') {
            return Ok((name.trim().to_string(), target.trim().to_string()));
        }

        Err(Error::Parse {
            line: self.pos,
            message: "invalid alias syntax".to_string(),
        })
    }

    fn is_recipe_header(&self, line: &str) -> bool {
        // A recipe header is: name [params]: [deps]
        // It must start with an identifier and contain a colon
        if line.is_empty() {
            return false;
        }

        let first_char = line.chars().next().unwrap();
        if !first_char.is_alphabetic() && first_char != '_' && first_char != '@' {
            return false;
        }

        // Must have a colon somewhere (but not :=)
        line.contains(':') && !line.contains(":=")
    }

    fn parse_recipe(&mut self) -> Result<Recipe> {
        let header = self.lines[self.pos].trim();
        self.pos += 1;

        // Handle @ prefix for quiet
        let (quiet, header) = if header.starts_with('@') {
            (true, &header[1..])
        } else {
            (false, header)
        };

        // Split on : to separate params from deps
        let (params_part, deps_part) = header
            .split_once(':')
            .ok_or_else(|| Error::Parse {
                line: self.pos,
                message: "recipe header missing colon".to_string(),
            })?;

        // Parse recipe name and parameters
        let mut parts = params_part.split_whitespace();
        let name = parts.next().unwrap().to_string();
        let parameters = self.parse_parameters(parts)?;

        // Parse dependencies
        let dependencies = self.parse_dependencies(deps_part.trim())?;

        // Parse body
        let (body, shebang) = self.parse_body()?;

        Ok(Recipe {
            name,
            doc: None,
            parameters,
            dependencies,
            body,
            quiet,
            shebang,
        })
    }

    fn parse_parameters<'b>(
        &self,
        parts: impl Iterator<Item = &'b str>,
    ) -> Result<Vec<Parameter>> {
        let mut params = Vec::new();

        for part in parts {
            let (kind, name_part) = if part.starts_with('*') {
                (ParameterKind::Variadic, &part[1..])
            } else if part.starts_with('+') {
                (ParameterKind::PlusVariadic, &part[1..])
            } else {
                (ParameterKind::Normal, part)
            };

            // Check for default value: NAME="default" or NAME='default'
            let (name, default) = if let Some((n, d)) = name_part.split_once('=') {
                (n.to_string(), Some(parse_quoted_value(d)))
            } else {
                (name_part.to_string(), None)
            };

            params.push(Parameter {
                name,
                kind,
                default,
            });
        }

        Ok(params)
    }

    fn parse_dependencies(&self, deps_str: &str) -> Result<Vec<Dependency>> {
        if deps_str.is_empty() {
            return Ok(Vec::new());
        }

        let mut deps = Vec::new();

        // Simple parsing: split on whitespace, each token is a dependency
        // More complex: dep(arg1, arg2) - not implementing for now
        for part in deps_str.split_whitespace() {
            deps.push(Dependency {
                recipe: part.to_string(),
                arguments: Vec::new(),
            });
        }

        Ok(deps)
    }

    fn parse_body(&mut self) -> Result<(Vec<Line>, Option<String>)> {
        let mut lines = Vec::new();
        let mut shebang = None;

        while self.pos < self.lines.len() {
            let line = self.lines[self.pos];

            // Completely empty lines continue the body (don't break on blank lines)
            if line.is_empty() {
                self.pos += 1;
                continue;
            }

            // Body lines must be indented (start with whitespace)
            if !line.starts_with(' ') && !line.starts_with('\t') {
                break;
            }

            self.pos += 1;

            // Get the content without leading indentation
            let content = line.trim_start();

            // Empty line in body
            if content.is_empty() {
                continue;
            }

            // Check for shebang on first line
            if lines.is_empty() && shebang.is_none() && content.starts_with("#!") {
                shebang = Some(content.to_string());
                continue;
            }

            // Parse the line
            lines.extend(self.parse_recipe_line(content));
        }

        Ok((lines, shebang))
    }

    fn parse_recipe_line(&self, content: &str) -> Vec<Line> {
        // Check for @ or - prefixes first
        let (quiet, content) = if content.starts_with('@') {
            (true, content[1..].trim_start())
        } else {
            (false, content)
        };

        let (ignore_errors, content) = if content.starts_with('-') {
            (true, content[1..].trim_start())
        } else {
            (false, content)
        };

        // Check for export VAR=value
        if let Some(rest) = content.strip_prefix("export ") {
            let (assignments, remainder) = split_leading_assignments(rest);
            if !assignments.is_empty() && remainder.is_empty() {
                return assignments
                    .into_iter()
                    .map(|(name, value)| Line::Export { name, value })
                    .collect();
            }
        }

        // A line of nothing but `VAR=value` words assigns them, and they persist
        // to later lines. Anything after them makes it an ordinary command with
        // an environment prefix — `FOO=bar cmd` scopes FOO to cmd, as in a
        // shell. Taking it as an assignment instead would swallow the command:
        // its value would be evaluated for its output, so the command's stderr
        // and exit status would go with it.
        let (assignments, remainder) = split_leading_assignments(content);
        if !assignments.is_empty() && remainder.is_empty() {
            return assignments
                .into_iter()
                .map(|(name, value)| Line::Assignment { name, value })
                .collect();
        }

        // Regular command
        vec![Line::Command(Command {
            text: content.to_string(),
            quiet,
            ignore_errors,
        })]
    }
}

/// Split the leading run of `NAME=VALUE` shell words off a line, returning them
/// and whatever text follows.
///
/// Values are kept verbatim, quotes and all, because the executor hands them
/// back to the shell to evaluate. Words are split on unquoted whitespace, so a
/// value may contain spaces when they are quoted (`BAR="x y"`) or inside a
/// command substitution (`BAR=$(echo x y)`).
pub fn split_leading_assignments(content: &str) -> (Vec<(String, String)>, &str) {
    let mut assignments = Vec::new();
    let mut rest = content.trim_start();

    while let Some((word, tail)) = next_word(rest) {
        let Some((name, value)) = word.split_once('=') else {
            break;
        };
        // `==` is a comparison, not an assignment.
        if !is_identifier(name) || value.starts_with('=') {
            break;
        }
        assignments.push((name.to_string(), value.to_string()));
        rest = tail;
    }

    (assignments, rest)
}

/// The next shell word in `s` and the text after it, or `None` if `s` is blank.
///
/// Quoting only has to be tracked well enough to know where the word ends:
/// unquoted whitespace separates words, and everything else is passed through
/// for the shell to interpret.
fn next_word(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }

    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut quote = None;
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                // A backslash escapes the next byte, but only inside double
                // quotes; single quotes are literal all the way through.
                if q == b'"' && c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'\\' => i += 1,
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'(' if i > 0 && bytes[i - 1] == b'$' => depth += 1,
                b'{' if i > 0 && bytes[i - 1] == b'$' => depth += 1,
                b')' | b'}' if depth > 0 => depth -= 1,
                _ if depth == 0 && c.is_ascii_whitespace() => break,
                _ => {}
            },
        }
        i += 1;
    }

    let end = i.min(s.len());
    Some((&s[..end], &s[end..]))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_quoted_value(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn parse_string_array(s: &str) -> Vec<String> {
    // Parse ["foo", "bar"] format
    let s = s.trim();
    let s = s.strip_prefix('[').unwrap_or(s);
    let s = s.strip_suffix(']').unwrap_or(s);

    s.split(',')
        .map(|part| parse_quoted_value(part.trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_recipe() {
        let input = r#"
build:
    cargo build
"#;
        let jf = parse(input).unwrap();
        assert_eq!(jf.recipes.len(), 1);
        assert_eq!(jf.recipes[0].name, "build");
    }

    #[test]
    fn test_recipe_with_params() {
        let input = r#"
test NAME *ARGS:
    cargo test $NAME $ARGS
"#;
        let jf = parse(input).unwrap();
        assert_eq!(jf.recipes.len(), 1);
        assert_eq!(jf.recipes[0].parameters.len(), 2);
        assert_eq!(jf.recipes[0].parameters[0].name, "NAME");
        assert_eq!(jf.recipes[0].parameters[1].kind, ParameterKind::Variadic);
    }

    #[test]
    fn test_export() {
        let input = r#"
export PATH="foo:$PATH"

build:
    echo $PATH
"#;
        let jf = parse(input).unwrap();
        assert_eq!(jf.exports.len(), 1);
        assert_eq!(jf.exports[0].name, "PATH");
        assert_eq!(jf.exports[0].value, "foo:$PATH");
    }

    #[test]
    fn test_settings() {
        let input = r#"
set next
set venv = ".venv"

build:
    python -m build
"#;
        let jf = parse(input).unwrap();
        assert!(jf.settings.next);
        assert_eq!(jf.settings.venv, Some(".venv".to_string()));
    }

    #[test]
    fn test_shebang_recipe_with_empty_lines() {
        // Regression test: empty lines (without indentation) within a shebang recipe
        // should not cause the parser to exit the recipe body early
        let input = r#"
run-linux:
    #!/usr/bin/env bash
    set -e
    mkdir -p target

    # Detect architecture
    ARCH=$(uname -m)
    case "$ARCH" in
        arm64|aarch64) TARGET="aarch64-linux-gnu" ;;
        x86_64)        TARGET="x86_64-linux-gnu" ;;
        *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac

    echo "Building for $TARGET"
"#;
        let jf = parse(input).unwrap();
        assert_eq!(jf.recipes.len(), 1);
        assert_eq!(jf.recipes[0].name, "run-linux");
        assert_eq!(
            jf.recipes[0].shebang,
            Some("#!/usr/bin/env bash".to_string())
        );
        // Should have parsed all the body lines (set -e, mkdir, ARCH=, case, esac, echo)
        assert!(jf.recipes[0].body.len() >= 6);
    }

    #[test]
    fn test_shebang_detection() {
        let input = r#"
build:
    #!/bin/sh
    echo "hello"
"#;
        let jf = parse(input).unwrap();
        assert_eq!(jf.recipes[0].shebang, Some("#!/bin/sh".to_string()));
        assert_eq!(jf.recipes[0].body.len(), 1);
    }
}
