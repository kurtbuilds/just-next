use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::process::{Command, Stdio};

use crate::v2::ast::*;
use crate::v2::environment::Environment;
use crate::v2::error::{Error, Result};

/// ANSI escape codes for bold text
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Echo a line to stderr, bold when a terminal is there to render it.
///
/// Redirected stderr is being read by something that wants the text — a log
/// file, a `2>&1 | grep`, a test harness comparing it byte for byte. Bolding
/// unconditionally writes `\x1b[1m` into all of them.
fn echo(line: &str) {
    if std::io::stderr().is_terminal() {
        eprintln!("{BOLD}{line}{RESET}");
    } else {
        eprintln!("{line}");
    }
}

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
        let mut ran = HashSet::new();
        self.run_recipe_with_deps(recipe_name, args, &mut visited, &mut ran)
    }

    fn run_recipe_with_deps(
        &self,
        recipe_name: &str,
        args: &[String],
        visited: &mut HashSet<String>,
        ran: &mut HashSet<(String, Vec<String>)>,
    ) -> Result<()> {
        // Check for circular dependencies
        if visited.contains(recipe_name) {
            return Err(Error::CircularDependency(recipe_name.to_string()));
        }

        // Find the recipe
        let recipe = self.find_recipe(recipe_name)?;

        // A recipe runs at most once per invocation for any one set of
        // arguments, so `foo: bar bar` runs bar once and a diamond runs the
        // shared dependency once. Keyed on the resolved name so an alias and
        // the recipe it points at are the same run.
        //
        // Not just a tidiness rule: dependencies form a DAG, not a tree, so
        // re-running a repeated one makes traversal exponential. A chain of 40
        // `r{i}: r{i+1} r{i+1}` recipes takes 2^40 runs without this.
        if !ran.insert((recipe.name.clone(), args.to_vec())) {
            return Ok(());
        }

        visited.insert(recipe_name.to_string());

        // Run dependencies first
        for dep in &recipe.dependencies {
            self.run_recipe_with_deps(&dep.recipe, &dep.arguments, visited, ran)?;
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
    ) -> Result<HashMap<String, Binding>> {
        let mut bindings = HashMap::new();
        let mut arg_iter = args.iter().peekable();

        for param in params {
            match param.kind {
                ParameterKind::Normal => {
                    if let Some(value) = arg_iter.next() {
                        bindings.insert(param.name.clone(), Binding::One(value.clone()));
                    } else if let Some(default) = &param.default {
                        bindings.insert(param.name.clone(), Binding::One(default.clone()));
                    } else {
                        return Err(Error::MissingArgument(param.name.clone()));
                    }
                }
                ParameterKind::Variadic => {
                    let remaining: Vec<_> = arg_iter.by_ref().cloned().collect();
                    bindings.insert(param.name.clone(), Binding::Many(remaining));
                }
                ParameterKind::PlusVariadic => {
                    let remaining: Vec<_> = arg_iter.by_ref().cloned().collect();
                    if remaining.is_empty() {
                        return Err(Error::MissingArgument(param.name.clone()));
                    }
                    bindings.insert(param.name.clone(), Binding::Many(remaining));
                }
            }
        }

        Ok(bindings)
    }

    fn execute_recipe(
        &self,
        recipe: &Recipe,
        bindings: HashMap<String, Binding>,
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
        bindings: &HashMap<String, Binding>,
    ) -> Result<()> {
        let shebang = recipe.shebang.as_ref().unwrap();

        // Build the complete script
        let mut script = String::new();
        script.push_str(shebang);
        script.push('\n');

        // Add parameter bindings. A shebang recipe is a single script, so the
        // shell itself handles quoting from here on.
        for (name, binding) in bindings {
            script.push_str(&format!("{}={}\n", name, shell_quote(&binding.joined())));
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
                        echo(&expanded);
                    }
                    return Ok(());
                }

                // Print the command in bold (unless quiet)
                if !self.quiet && !cmd.quiet && !recipe_quiet {
                    echo(&expanded);
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
                let final_value = self.evaluate_assignment(name, value, state)?;

                if !self.quiet {
                    echo(&format!("{name}={final_value}"));
                }

                state.set_var(name.clone(), final_value);
            }
            Line::Export { name, value } => {
                let final_value = self.evaluate_assignment(name, value, state)?;

                if !self.quiet {
                    echo(&format!("export {name}={final_value}"));
                }

                state.set_var(name.clone(), final_value.clone());
                state.export_var(name.clone(), final_value);
            }
        }

        Ok(())
    }

    /// Evaluate the right-hand side of a recipe-body assignment.
    ///
    /// The assignment is performed by the shell and the result read back, so it
    /// means exactly what the same line would mean in a shell script: quoting,
    /// variable references and command substitution all behave normally.
    /// Variables set by earlier lines are in the environment, so they resolve.
    fn evaluate_assignment(
        &self,
        name: &str,
        value: &str,
        state: &ExecutionState,
    ) -> Result<String> {
        let expanded = state.expand_command(value);

        if self.dry_run {
            return Ok(expanded);
        }

        self.evaluate_expression(&format!("{name}={expanded}\nprintf '%s' \"${name}\""), state)
    }

    fn evaluate_expression(&self, expr: &str, state: &ExecutionState) -> Result<String> {
        if self.dry_run {
            return Ok(expr.to_string());
        }

        // Run the expression in bash and capture output
        let output = Command::new("bash")
            .arg("-c")
            .arg(expr)
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

/// The value bound to a recipe parameter.
///
/// Variadic parameters keep their arguments as separate items rather than a
/// joined string, so that each can be quoted independently when substituted.
#[derive(Debug, Clone)]
pub enum Binding {
    One(String),
    Many(Vec<String>),
}

impl Binding {
    /// The value as a single string, for passing through the environment.
    fn joined(&self) -> String {
        match self {
            Binding::One(value) => value.clone(),
            Binding::Many(values) => values.join(" "),
        }
    }

    /// The value as it should appear in an unquoted position: every item
    /// shell-quoted, so arguments survive word splitting and globbing intact.
    fn quoted(&self) -> String {
        match self {
            Binding::One(value) => shell_quote(value),
            Binding::Many(values) => values
                .iter()
                .map(|value| shell_quote(value))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

/// Execution state that tracks variables, exports, and positional args across lines
struct ExecutionState<'a> {
    base_env: &'a Environment,
    /// Recipe parameters, substituted textually with automatic quoting.
    params: HashMap<String, Binding>,
    /// Variables assigned by recipe body lines, passed through the environment.
    vars: HashMap<String, String>,
    exports: HashMap<String, String>,
    positional_args: Vec<String>,
}

impl<'a> ExecutionState<'a> {
    fn new(
        base_env: &'a Environment,
        params: HashMap<String, Binding>,
        args: Vec<String>,
    ) -> Self {
        Self {
            base_env,
            params,
            vars: HashMap::new(),
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

        // Parameters are visible to child processes too, joined the way just
        // presents them.
        for (name, binding) in &self.params {
            env.insert(name.clone(), binding.joined());
        }

        // Variables assigned by earlier body lines. These are left to the shell
        // to expand rather than substituted, so ordinary shell semantics apply.
        for (name, value) in &self.vars {
            env.insert(name.clone(), value.clone());
        }

        for (name, value) in &self.exports {
            env.insert(name.clone(), value.clone());
        }

        env
    }

    /// Substitute recipe parameters and positional arguments into a command.
    ///
    /// Substitution is quoting-aware, which is what makes `./program $NAME
    /// $ARGS` work with arguments that contain spaces:
    ///
    /// * In an unquoted position, each value is shell-quoted, so it survives
    ///   word splitting and globbing as one argument. A variadic parameter
    ///   expands to its items individually quoted.
    /// * Inside double quotes, the raw value is inserted — the quotes the user
    ///   wrote already do the job, and quoting again would be visible in the
    ///   output.
    /// * Inside single quotes, nothing is substituted, matching the shell.
    ///
    /// Anything that is not a parameter or a positional argument is left alone
    /// for the shell to expand; those values are passed through the environment.
    ///
    /// Quoting is tracked with a single-pass scanner, so constructs that open a
    /// fresh quoting context — command substitution, most notably — are
    /// approximated rather than parsed.
    fn expand_command(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        let mut in_single = false;
        let mut in_double = false;

        while let Some(c) = chars.next() {
            match c {
                '\\' if !in_single => {
                    result.push(c);
                    if let Some(escaped) = chars.next() {
                        result.push(escaped);
                    }
                }
                '\'' if !in_double => {
                    in_single = !in_single;
                    result.push(c);
                }
                '"' if !in_single => {
                    in_double = !in_double;
                    result.push(c);
                }
                '$' if !in_single => {
                    match self.read_substitution(&mut chars) {
                        Some((binding, raw)) => {
                            // `"$@"` is the shell's "each argument separately"
                            // form, so it stays a quoted list even inside quotes.
                            let list_in_quotes = in_double && raw == "@";
                            if in_double && !list_in_quotes {
                                result.push_str(&binding.joined());
                            } else if list_in_quotes {
                                // Close, splice, reopen, so the result is still
                                // one well-formed double-quoted region.
                                result.push('"');
                                result.push_str(&binding.quoted());
                                result.push('"');
                            } else {
                                result.push_str(&binding.quoted());
                            }
                        }
                        None => result.push('$'),
                    }
                }
                _ => result.push(c),
            }
        }

        result
    }

    /// Read a variable reference following a `$`, returning its value and the
    /// name as written.
    ///
    /// Returns `None`, consuming nothing, when the reference is not something
    /// this engine substitutes — those are left for the shell.
    fn read_substitution(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Option<(Binding, String)> {
        let braced = chars.peek() == Some(&'{');

        // Peek the name without consuming, so a non-match leaves the input
        // untouched for the shell.
        let mut lookahead = chars.clone();
        if braced {
            lookahead.next();
        }

        let mut name = String::new();
        while let Some(&c) = lookahead.peek() {
            if c.is_alphanumeric() || c == '_' {
                name.push(c);
                lookahead.next();
            } else {
                break;
            }
        }

        if name.is_empty() {
            // `$@` and `$*` expand to every positional argument.
            if !braced && matches!(chars.peek(), Some('@') | Some('*')) {
                let symbol = chars.next().unwrap();
                return Some((
                    Binding::Many(self.positional_args.clone()),
                    symbol.to_string(),
                ));
            }
            return None;
        }

        if braced {
            // Only a plain `${NAME}` is ours; `${NAME:-default}` and friends
            // belong to the shell.
            if lookahead.peek() != Some(&'}') {
                return None;
            }
            lookahead.next();
        }

        let binding = self.lookup(&name)?;

        *chars = lookahead;
        Some((binding, name))
    }

    /// The value of a name this engine substitutes, if it is one.
    fn lookup(&self, name: &str) -> Option<Binding> {
        if let Some(binding) = self.params.get(name) {
            return Some(binding.clone());
        }

        // Positional arguments: `$1`, `$2`, ...
        if let Ok(index) = name.parse::<usize>() {
            if index >= 1 {
                return Some(Binding::One(
                    self.positional_args
                        .get(index - 1)
                        .cloned()
                        .unwrap_or_default(),
                ));
            }
        }

        None
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

}
