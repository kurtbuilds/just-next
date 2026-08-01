//! The command-line entry point for the V2 engine.
//!
//! Reached only when [`crate::dispatch`] has already identified the justfile as
//! V2. V1 invocations never get here — they go straight to `just_v1::run` with
//! argv untouched.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::dispatch;
use crate::search;
use crate::v2::environment::Environment;
use crate::v2::error::{Error, Result};
use crate::v2::executor::{list_recipes, Executor};
use crate::v2::parser;

#[derive(Parser)]
#[command(name = "just", version, about = "A modern command runner")]
struct Cli {
    /// Print what would be done without executing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Suppress all output
    #[arg(short, long)]
    quiet: bool,

    /// Use specific justfile
    #[arg(short = 'f', long)]
    justfile: Option<PathBuf>,

    /// Set working directory
    #[arg(short = 'd', long)]
    working_directory: Option<PathBuf>,

    /// List available recipes
    #[arg(short, long)]
    list: bool,

    /// Force next-style parsing, skipping detection
    #[arg(long)]
    next: bool,

    /// Force the vendored upstream `just` engine
    #[arg(long)]
    legacy: bool,

    /// Recipe and arguments
    #[arg(trailing_var_arg = true)]
    arguments: Vec<String>,
}

/// Run a V2 justfile.
pub fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let working_dir = cli
        .working_directory
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // `just <folder>/<recipe>` runs a recipe from the justfile in <folder>.
    let mut arguments = cli.arguments.clone();
    let mut folder_justfile = None;
    if cli.justfile.is_none() {
        if let Some((dir, recipe)) = split_folder_recipe(arguments.first().map(String::as_str)) {
            let dir = working_dir.join(dir);
            if dir.is_dir() {
                folder_justfile = Some(
                    search::justfile_in(&dir).ok_or_else(|| Error::NoJustfileInDir(dir.clone()))?,
                );
                if recipe.is_empty() {
                    // `just <folder>/` runs that folder's default recipe.
                    arguments.remove(0);
                } else {
                    arguments[0] = recipe.to_string();
                }
            }
        }
    }

    let justfile_path = match folder_justfile {
        Some(path) => path,
        None => find_justfile(cli.justfile.as_deref(), &working_dir)?,
    };

    let content = std::fs::read_to_string(&justfile_path).map_err(|source| Error::ReadFailed {
        path: justfile_path.clone(),
        source,
    })?;

    let justfile = parser::parse(&content)?;

    let justfile_dir = justfile_path.parent().unwrap_or(&working_dir);
    let mut env = Environment::new(justfile_dir.to_path_buf());
    env.setup(&justfile.settings, &justfile.exports);

    if cli.list {
        list_recipes(&justfile);
        return Ok(());
    }

    let (recipe_name, args) = if arguments.is_empty() {
        match justfile.recipes.first() {
            Some(first) => (first.name.clone(), Vec::new()),
            None => {
                eprintln!("No recipes found in justfile");
                return Ok(());
            }
        }
    } else {
        (arguments[0].clone(), arguments[1..].to_vec())
    };

    Executor::new(&justfile, &env)
        .dry_run(cli.dry_run)
        .quiet(cli.quiet)
        .run(&recipe_name, &args)
}

/// Split a `<folder>/<recipe>` argument into its directory and recipe parts.
///
/// Returns `None` when the argument has no `/`, i.e. it is a plain recipe name.
fn split_folder_recipe(argument: Option<&str>) -> Option<(&str, &str)> {
    let (dir, recipe) = dispatch::split_folder_argument(argument?)?;
    Some((dir, recipe))
}

fn find_justfile(specified: Option<&std::path::Path>, working_dir: &PathBuf) -> Result<PathBuf> {
    if let Some(path) = specified {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(Error::ReadFailed {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        });
    }

    search::justfile_from(working_dir).ok_or(Error::NoJustfile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_folder_scoped_recipes() {
        assert_eq!(split_folder_recipe(Some("build")), None);
        assert_eq!(split_folder_recipe(None), None);
        assert_eq!(split_folder_recipe(Some("/build")), None);
        assert_eq!(split_folder_recipe(Some("api/build")), Some(("api", "build")));
        assert_eq!(split_folder_recipe(Some("api/")), Some(("api", "")));
        assert_eq!(
            split_folder_recipe(Some("crates/api/build")),
            Some(("crates/api", "build"))
        );
        assert_eq!(
            split_folder_recipe(Some("../api/build")),
            Some(("../api", "build"))
        );
    }
}
