use std::collections::{HashMap, HashSet};
use std::process::{Command, Stdio};

use crate::ast::*;
use crate::environment::Environment;
use crate::error::{Error, Result};

/// ANSI escape codes for bold text
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

pub struct Executor<'a> {
    justfile: &'a Justfile,
    env: &'a Environment,
    dry_run: bool,
    quiet: bool,
}

impl<'a> Executor<'a> {
    pub fn new(justfile: &'a Justfile, env: &'a Environment) -> Self {
        Self {
            justfile,
            env,
            dry_run: false,
            quiet: false,
        }
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Run a recipe by name with given arguments
    pub fn run(&self, recipe_name: &str, args: &[String]) -> Result<()> {
        let mut visited = HashSet::new();
        self.run_recipe_with_deps(recipe_name, args, &mut visited)
    }

    fn run_recipe_with_deps(
        &self,
        recipe_name: &str,
        args: &[String],
        visited: &mut HashSet<String>,
    ) -> Result<()> {
        // Check for circular dependencies
        if visited.contains(recipe_name) {
            return Err(Error::CircularDependency(recipe_name.to_string()));
        }
        visited.insert(recipe_name.to_string());

        // Find the recipe
        let recipe = self.find_recipe(recipe_name)?;

        // Run dependencies first
        for dep in &recipe.dependencies {
            self.run_recipe_with_deps(&dep.recipe, &dep.arguments, visited)?;
        }

        // Bind parameters to arguments
        let bindings = self.bind_parameters(&recipe.parameters, args)?;

        // Execute the recipe
        self.execute_recipe(recipe, bindings, args)?;

        visited.remove(recipe_name);
        Ok(())
    }

    fn find_recipe(&self, name: &str) -> Result<&Recipe> {
        // Check recipes directly
        if let Some(recipe) = self.justfile.recipes.iter().find(|r| r.name == name) {
            return Ok(recipe);
        }

        // Check aliases
        if let Some(target) = self.justfile.aliases.get(name) {
            if let Some(recipe) = self.justfile.recipes.iter().find(|r| r.name == *target) {
                return Ok(recipe);
            }
        }

        Err(Error::RecipeNotFound(name.to_string()))
    }

    fn bind_parameters(
        &self,
        params: &[Parameter],
        args: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut bindings = HashMap::new();
        let mut arg_iter = args.iter().peekable();

        for param in params {
            match param.kind {
                ParameterKind::Normal => {
                    if let Some(value) = arg_iter.next() {
                        bindings.insert(param.name.clone(), value.clone());
                    } else if let Some(default) = &param.default {
                        bindings.insert(param.name.clone(), default.clone());
                    } else {
                        return Err(Error::MissingArgument(param.name.clone()));
                    }
                }
                ParameterKind::Variadic => {
                    let remaining: Vec<_> = arg_iter.by_ref().cloned().collect();
                    bindings.insert(param.name.clone(), remaining.join(" "));
                }
                ParameterKind::PlusVariadic => {
                    let remaining: Vec<_> = arg_iter.by_ref().cloned().collect();
                    if remaining.is_empty() {
                        return Err(Error::MissingArgument(param.name.clone()));
                    }
                    bindings.insert(param.name.clone(), remaining.join(" "));
                }
            }
        }

        Ok(bindings)
    }

    fn execute_recipe(
        &self,
        recipe: &Recipe,
        bindings: HashMap<String, String>,
        original_args: &[String],
    ) -> Result<()> {
        if recipe.body.is_empty() {
            return Ok(());
        }

        // If recipe has a shebang, execute as a single script
        if recipe.shebang.is_some() {
            return self.execute_shebang_recipe(recipe, &bindings);
        }

        // Create execution state
        let mut state = ExecutionState::new(self.env, bindings, original_args.to_vec());

        // Execute each line
        for line in &recipe.body {
            self.execute_line(line, &mut state, recipe.quiet)?;
        }

        Ok(())
    }

    fn execute_shebang_recipe(
        &self,
        recipe: &Recipe,
        bindings: &HashMap<String, String>,
    ) -> Result<()> {
        let shebang = recipe.shebang.as_ref().unwrap();

        // Build the complete script
        let mut script = String::new();
        script.push_str(shebang);
        script.push('\n');

        // Add parameter bindings
        for (name, value) in bindings {
            script.push_str(&format!("{}={}\n", name, shell_quote(value)));
        }

        // Add recipe body
        for line in &recipe.body {
            match line {
                Line::Command(cmd) => {
                    script.push_str(&cmd.text);
                    script.push('\n');
                }
                Line::Assignment { name, value } => {
                    script.push_str(&format!("{}={}\n", name, value));
                }
                Line::Export { name, value } => {
                    script.push_str(&format!("export {}={}\n", name, value));
                }
            }
        }

        if self.dry_run {
            println!("{}", script);
            return Ok(());
        }

        // Execute via bash
        let status = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .current_dir(self.env.working_dir())
            .envs(self.env.vars())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(Error::ExecFailed)?;

        if !status.success() {
            return Err(Error::RecipeFailed(
                recipe.name.clone(),
                status.code().unwrap_or(1),
            ));
        }

        Ok(())
    }

    fn execute_line(
        &self,
        line: &Line,
        state: &mut ExecutionState,
        recipe_quiet: bool,
    ) -> Result<()> {
        match line {
            Line::Command(cmd) => {
                let expanded = state.expand_command(&cmd.text);

                // Check for shift command
                if expanded.trim() == "shift" || expanded.trim().starts_with("shift ") {
                    let n = expanded
                        .trim()
                        .strip_prefix("shift")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                        .unwrap_or(1);
                    state.shift(n);

                    if !self.quiet && !cmd.quiet && !recipe_quiet {
                        eprintln!("{}{}{}", BOLD, expanded, RESET);
                    }
                    return Ok(());
                }

                // Print the command in bold (unless quiet)
                if !self.quiet && !cmd.quiet && !recipe_quiet {
                    eprintln!("{}{}{}", BOLD, expanded, RESET);
                }

                if self.dry_run {
                    return Ok(());
                }

                // Execute the command
                let status = Command::new("bash")
                    .arg("-c")
                    .arg(&expanded)
                    .current_dir(self.env.working_dir())
                    .envs(state.env_vars())
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .map_err(Error::ExecFailed)?;

                if !status.success() && !cmd.ignore_errors {
                    return Err(Error::RecipeFailed(
                        "command".to_string(),
                        status.code().unwrap_or(1),
                    ));
                }
            }
            Line::Assignment { name, value } => {
                // Expand and evaluate the value
                let expanded = state.expand_command(value);

                // Check if it's a command substitution $(...)
                let final_value = if expanded.contains("$(") || expanded.contains("`") {
                    // Evaluate the expression
                    self.evaluate_expression(&expanded, state)?
                } else {
                    // Just use the expanded value (strip quotes if present)
                    strip_quotes(&expanded)
                };

                if !self.quiet {
                    eprintln!("{}{}={}{}", BOLD, name, final_value, RESET);
                }

                state.set_var(name.clone(), final_value);
            }
            Line::Export { name, value } => {
                let expanded = state.expand_command(value);

                let final_value = if expanded.contains("$(") || expanded.contains("`") {
                    self.evaluate_expression(&expanded, state)?
                } else {
                    strip_quotes(&expanded)
                };

                if !self.quiet {
                    eprintln!("{}export {}={}{}", BOLD, name, final_value, RESET);
                }

                state.set_var(name.clone(), final_value.clone());
                state.export_var(name.clone(), final_value);
            }
        }

        Ok(())
    }

    fn evaluate_expression(&self, expr: &str, state: &ExecutionState) -> Result<String> {
        if self.dry_run {
            return Ok(expr.to_string());
        }

        // Run the expression in bash and capture output
        let output = Command::new("bash")
            .arg("-c")
            .arg(format!("echo -n {}", expr))
            .current_dir(self.env.working_dir())
            .envs(state.env_vars())
            .output()
            .map_err(Error::ExecFailed)?;

        if !output.status.success() {
            return Err(Error::RecipeFailed(
                "expression".to_string(),
                output.status.code().unwrap_or(1),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Execution state that tracks variables, exports, and positional args across lines
struct ExecutionState<'a> {
    base_env: &'a Environment,
    vars: HashMap<String, String>,
    exports: HashMap<String, String>,
    positional_args: Vec<String>,
}

impl<'a> ExecutionState<'a> {
    fn new(
        base_env: &'a Environment,
        bindings: HashMap<String, String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            base_env,
            vars: bindings,
            exports: HashMap::new(),
            positional_args: args,
        }
    }

    fn set_var(&mut self, name: String, value: String) {
        self.vars.insert(name, value);
    }

    fn export_var(&mut self, name: String, value: String) {
        self.exports.insert(name, value);
    }

    fn shift(&mut self, n: usize) {
        if n <= self.positional_args.len() {
            self.positional_args = self.positional_args[n..].to_vec();
        } else {
            self.positional_args.clear();
        }
    }

    fn env_vars(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = self.base_env.vars().clone();

        // Add local variables
        for (k, v) in &self.vars {
            env.insert(k.clone(), v.clone());
        }

        // Add exports
        for (k, v) in &self.exports {
            env.insert(k.clone(), v.clone());
        }

        env
    }

    fn expand_command(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Replace $@ with all positional args (quoted)
        if result.contains("\"$@\"") {
            let quoted_args: Vec<String> = self
                .positional_args
                .iter()
                .map(|a| shell_quote(a))
                .collect();
            result = result.replace("\"$@\"", &quoted_args.join(" "));
        }
        if result.contains("$@") {
            result = result.replace("$@", &self.positional_args.join(" "));
        }

        // Replace $* with all positional args
        if result.contains("$*") {
            result = result.replace("$*", &self.positional_args.join(" "));
        }

        // Replace $1, $2, etc.
        for (i, arg) in self.positional_args.iter().enumerate() {
            let var = format!("${}", i + 1);
            result = result.replace(&var, arg);
        }

        // Replace $VAR with variable values
        for (name, value) in &self.vars {
            result = result.replace(&format!("${}", name), value);
            result = result.replace(&format!("${{{}}}", name), value);
        }

        // Replace environment variables
        for (name, value) in self.base_env.vars() {
            result = result.replace(&format!("${}", name), value);
            result = result.replace(&format!("${{{}}}", name), value);
        }

        result
    }
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// List available recipes
pub fn list_recipes(justfile: &Justfile) {
    println!("Available recipes:");
    for recipe in &justfile.recipes {
        let params: Vec<String> = recipe
            .parameters
            .iter()
            .map(|p| {
                let prefix = match p.kind {
                    ParameterKind::Variadic => "*",
                    ParameterKind::PlusVariadic => "+",
                    ParameterKind::Normal => "",
                };
                if let Some(default) = &p.default {
                    format!("{}{}='{}'", prefix, p.name, default)
                } else {
                    format!("{}{}", prefix, p.name)
                }
            })
            .collect();

        let params_str = if params.is_empty() {
            String::new()
        } else {
            format!(" {}", params.join(" "))
        };

        if let Some(doc) = &recipe.doc {
            println!("    {}{} # {}", recipe.name, params_str, doc);
        } else {
            println!("    {}{}", recipe.name, params_str);
        }
    }

    if !justfile.aliases.is_empty() {
        println!();
        println!("Aliases:");
        for (alias, target) in &justfile.aliases {
            println!("    {} := {}", alias, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_quote() {
        assert_eq!(shell_quote("simple"), "simple");
        assert_eq!(shell_quote("with space"), "'with space'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'world'"), "world");
        assert_eq!(strip_quotes("plain"), "plain");
    }
}
