use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

mod ast;
mod detection;
mod environment;
mod error;
mod executor;
mod parser;

use detection::{detect_style, exec_legacy_just, JustfileStyle};
use environment::Environment;
use error::Error;
use executor::{list_recipes, Executor};

#[derive(Parser)]
#[command(name = "just-next", version, about = "A modern command runner")]
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

    /// Force next mode (skip legacy detection)
    #[arg(long)]
    next: bool,

    /// Force legacy mode (use original just)
    #[arg(long)]
    legacy: bool,

    /// Recipe and arguments
    #[arg(trailing_var_arg = true)]
    arguments: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> error::Result<()> {
    let cli = Cli::parse();

    // Determine working directory
    let working_dir = cli
        .working_directory
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // `just-next <folder>/<recipe>` runs a recipe from the justfile in <folder>
    let mut arguments = cli.arguments.clone();
    let mut folder_justfile = None;
    if cli.justfile.is_none() {
        if let Some((dir, recipe)) = split_folder_recipe(arguments.first().map(|s| s.as_str())) {
            let dir = working_dir.join(dir);
            if dir.is_dir() {
                folder_justfile = Some(find_justfile_in_dir(&dir)?);
                if recipe.is_empty() {
                    // `just-next <folder>/` runs the folder's default recipe
                    arguments.remove(0);
                } else {
                    arguments[0] = recipe.to_string();
                }
            }
        }
    }

    // Find justfile
    let is_folder_scoped = folder_justfile.is_some();
    let justfile_path = match folder_justfile {
        Some(path) => path,
        None => find_justfile(cli.justfile.as_deref(), &working_dir)?,
    };

    // Read justfile content
    let content = std::fs::read_to_string(&justfile_path).map_err(|e| Error::ReadFailed {
        path: justfile_path.clone(),
        source: e,
    })?;

    // Arguments to forward when delegating to the legacy just binary. Folder-scoped
    // invocations are rewritten, since legacy just has no `<folder>/<recipe>` syntax.
    let legacy_args = |cli: &Cli| -> Option<Vec<String>> {
        if !is_folder_scoped {
            return None;
        }
        let mut args = vec![
            "--justfile".to_string(),
            justfile_path.display().to_string(),
            "--working-directory".to_string(),
            justfile_path
                .parent()
                .unwrap_or(&working_dir)
                .display()
                .to_string(),
        ];
        if cli.dry_run {
            args.push("--dry-run".to_string());
        }
        if cli.quiet {
            args.push("--quiet".to_string());
        }
        if cli.list {
            args.push("--list".to_string());
        }
        args.extend(arguments.iter().cloned());
        Some(args)
    };

    // Force legacy mode if requested
    if cli.legacy {
        exec_legacy_just(None, legacy_args(&cli));
    }

    // Detect style unless forced to next
    if !cli.next {
        let style = detect_style(&content);
        if style == JustfileStyle::Legacy {
            // Parse to get the just path setting if any
            let just_path = parser::parse(&content).ok().and_then(|jf| jf.settings.just);
            exec_legacy_just(just_path.as_deref(), legacy_args(&cli));
        }
    }

    // Parse the justfile
    let justfile = parser::parse(&content)?;

    // Check for set next after parsing - it might force next mode
    // (already detected above, but let's respect explicit setting)

    // Set up environment
    let justfile_dir = justfile_path.parent().unwrap_or(&working_dir);
    let mut env = Environment::new(justfile_dir.to_path_buf());
    env.setup(&justfile.settings, &justfile.exports);

    // Handle --list
    if cli.list {
        list_recipes(&justfile);
        return Ok(());
    }

    // Determine recipe to run
    let (recipe_name, args) = if arguments.is_empty() {
        // Run first recipe
        if let Some(first) = justfile.recipes.first() {
            (first.name.clone(), vec![])
        } else {
            eprintln!("No recipes found in justfile");
            return Ok(());
        }
    } else {
        let recipe = arguments[0].clone();
        let args = arguments[1..].to_vec();
        (recipe, args)
    };

    // Execute
    let executor = Executor::new(&justfile, &env)
        .dry_run(cli.dry_run)
        .quiet(cli.quiet);

    executor.run(&recipe_name, &args)?;

    Ok(())
}

/// Names we accept for a justfile, in search order
const JUSTFILE_NAMES: &[&str] = &["justfile", "Justfile", ".justfile"];

/// Split a `<folder>/<recipe>` argument into its directory and recipe parts.
///
/// Returns `None` when the argument has no `/`, i.e. it is a plain recipe name.
/// The recipe part is empty for a trailing slash (`<folder>/`), which means "the
/// folder's default recipe".
fn split_folder_recipe(argument: Option<&str>) -> Option<(&str, &str)> {
    let argument = argument?;
    let (dir, recipe) = argument.rsplit_once('/')?;
    if dir.is_empty() {
        // A leading slash is an absolute path, not a folder/recipe pair
        return None;
    }
    Some((dir, recipe))
}

/// Find a justfile directly inside `dir`, without walking up the tree
fn find_justfile_in_dir(dir: &std::path::Path) -> error::Result<PathBuf> {
    for name in JUSTFILE_NAMES {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(Error::NoJustfileInDir(dir.to_path_buf()))
}

fn find_justfile(specified: Option<&std::path::Path>, working_dir: &PathBuf) -> error::Result<PathBuf> {
    // Use specified path if provided
    if let Some(path) = specified {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(Error::ReadFailed {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        });
    }

    // Search up the directory tree
    let mut current = working_dir.as_path();
    loop {
        for name in JUSTFILE_NAMES {
            let candidate = current.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    Err(Error::NoJustfile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_folder_recipe() {
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

    #[test]
    fn test_find_justfile_in_dir() {
        let dir = std::env::temp_dir().join("just-next-test-find-justfile-in-dir");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("Justfile"), "build:\n    true\n").unwrap();

        // Note: the name may come back as "justfile" on case-insensitive filesystems
        assert_eq!(find_justfile_in_dir(&dir).unwrap().parent(), Some(&*dir));

        // Does not walk up to the parent directory
        assert!(matches!(
            find_justfile_in_dir(&nested),
            Err(Error::NoJustfileInDir(_))
        ));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
