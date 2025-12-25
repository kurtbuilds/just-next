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

    // Find justfile
    let justfile_path = find_justfile(cli.justfile.as_deref(), &working_dir)?;

    // Read justfile content
    let content = std::fs::read_to_string(&justfile_path).map_err(|e| Error::ReadFailed {
        path: justfile_path.clone(),
        source: e,
    })?;

    // Force legacy mode if requested
    if cli.legacy {
        exec_legacy_just(None);
    }

    // Detect style unless forced to next
    if !cli.next {
        let style = detect_style(&content);
        if style == JustfileStyle::Legacy {
            // Parse to get the just path setting if any
            if let Ok(jf) = parser::parse(&content) {
                exec_legacy_just(jf.settings.just.as_deref());
            } else {
                exec_legacy_just(None);
            }
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
    let (recipe_name, args) = if cli.arguments.is_empty() {
        // Run first recipe
        if let Some(first) = justfile.recipes.first() {
            (first.name.clone(), vec![])
        } else {
            eprintln!("No recipes found in justfile");
            return Ok(());
        }
    } else {
        let recipe = cli.arguments[0].clone();
        let args = cli.arguments[1..].to_vec();
        (recipe, args)
    };

    // Execute
    let executor = Executor::new(&justfile, &env)
        .dry_run(cli.dry_run)
        .quiet(cli.quiet);

    executor.run(&recipe_name, &args)?;

    Ok(())
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
        for name in &["justfile", "Justfile", ".justfile"] {
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
