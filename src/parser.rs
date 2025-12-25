use crate::ast::*;
use crate::error::{Error, Result};

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
                "just" => settings.just = Some(value.to_string()),
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
            let parsed_line = self.parse_recipe_line(content);
            lines.push(parsed_line);
        }

        Ok((lines, shebang))
    }

    fn parse_recipe_line(&self, content: &str) -> Line {
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
            if let Some((name, value)) = rest.split_once('=') {
                return Line::Export {
                    name: name.trim().to_string(),
                    value: value.to_string(),
                };
            }
        }

        // Check for VAR=value (but not VAR==something or commands with = in them)
        // This is tricky - we want FOO=bar but not echo foo=bar
        // Heuristic: if it starts with IDENTIFIER= and no spaces before =, it's an assignment
        if let Some(eq_pos) = content.find('=') {
            let before_eq = &content[..eq_pos];
            // Check if it's a valid variable name (no spaces, starts with letter/_)
            if !before_eq.contains(' ')
                && !before_eq.is_empty()
                && (before_eq.chars().next().unwrap().is_alphabetic()
                    || before_eq.starts_with('_'))
                && before_eq.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                // Check it's not == (comparison)
                if !content[eq_pos..].starts_with("==") {
                    return Line::Assignment {
                        name: before_eq.to_string(),
                        value: content[eq_pos + 1..].to_string(),
                    };
                }
            }
        }

        // Regular command
        Line::Command(Command {
            text: content.to_string(),
            quiet,
            ignore_errors,
        })
    }
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
}
