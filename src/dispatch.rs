//! Deciding which engine parses a justfile.
//!
//! There are exactly two parsing paths in this binary:
//!
//! * [`Engine::V1`] — upstream `just`, vendored as `just_v1`. Byte-for-byte
//!   compatible with the real thing.
//! * [`Engine::V2`] — just-next's own [`crate::v2`] parser.
//!
//! The two grammars overlap heavily, so most justfiles are *ambiguous*: they
//! parse under both. `detect` therefore looks for constructs that are legal in
//! exactly one dialect, and falls back to [`AMBIGUOUS_DEFAULT`] when it finds
//! none.
//!
//! # Precedence
//!
//! 1. `set next` — explicit opt-in, always V2.
//! 2. Any **V1 signal** — V1. These win over V2 signals, because a justfile
//!    with `foo := "bar"` is legacy even if a recipe body happens to contain a
//!    line that looks like a V2 assignment.
//! 3. Any **V2 signal** — V2.
//! 4. Otherwise — [`AMBIGUOUS_DEFAULT`].
//!
//! # Why V1 signals are conservative
//!
//! Detection is failure-asymmetric. Misrouting a V1 justfile to V2 breaks a
//! working justfile; misrouting an ambiguous file to V1 merely forgoes
//! just-next's automatic environment setup. So the V1 signal set is broad and
//! the V2 signal set is narrow, and anything unreadable or unlocatable falls
//! back to V1.

use crate::{search, v2::parser::split_leading_assignments};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Which parsing path a justfile takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Upstream `just`, vendored as `just_v1`.
    V1,
    /// just-next's own parser.
    V2,
}

/// Engine for justfiles that parse identically under both dialects.
///
/// V1, so that a plain justfile behaves exactly as it does under upstream
/// `just`. The cost is that such a file does not get just-next's automatic
/// `.env` / `PATH` / virtualenv setup without a V2 signal or `set next`.
pub const AMBIGUOUS_DEFAULT: Engine = Engine::V1;

/// Decide which engine should parse `content`.
pub fn detect(content: &str) -> Engine {
    detect_with(content, AMBIGUOUS_DEFAULT)
}

/// [`detect`], with an explicit fallback for ambiguous justfiles.
pub fn detect_with(content: &str, ambiguous: Engine) -> Engine {
    let mut saw_v2 = false;

    // Under `set export`, V1 puts every variable and recipe parameter into the
    // environment, so `$PARAM` in a body is ordinary V1. Both dialects accept
    // the setting, so it is not a V1 signal on its own — it just disarms the
    // parameter-reference signal below.
    let exports_all = content.lines().any(|line| {
        let line = line.trim();
        line == "set export" || line.starts_with("set export ") || line.starts_with("set export=")
    });

    // Parameters of the recipe whose body we are currently inside, used to spot
    // `$PARAM` references — a V2-only way to reference a recipe parameter.
    let mut params: Vec<String> = Vec::new();
    let mut in_recipe = false;
    let mut body_started = false;
    // Shebang recipes already run as a single script under V1, so the gotchas
    // V2's body signals look for do not apply to them.
    let mut shebang_recipe = false;

    for raw in content.lines() {
        let indented = raw.starts_with(' ') || raw.starts_with('\t');
        let line = raw.trim();

        if line.is_empty() {
            continue;
        }

        if indented && in_recipe {
            if !body_started {
                body_started = true;
                shebang_recipe = line.starts_with("#!");
            }

            // Comments in a body are shell comments; they say nothing about
            // which dialect the justfile is written in.
            if shebang_recipe || line.starts_with('#') {
                continue;
            }

            let visible_params: &[String] = if exports_all { &[] } else { &params };
            if body_is_v2(line, visible_params) {
                saw_v2 = true;
            }
            continue;
        }

        if !indented {
            in_recipe = false;
            body_started = false;
            shebang_recipe = false;
            params.clear();
        }

        if line.starts_with('#') {
            continue;
        }

        if line == "set next" || line.starts_with("set next ") {
            return Engine::V2;
        }

        if top_level_is_v1(line) {
            return Engine::V1;
        }

        if top_level_is_v2(line) {
            saw_v2 = true;
        }

        if let Some(found) = recipe_header_params(line) {
            in_recipe = true;
            params = found;
        }
    }

    if saw_v2 { Engine::V2 } else { ambiguous }
}

/// The justfile a command line refers to, and the engine that should parse it.
pub struct Route {
    /// The engine to use.
    pub engine: Engine,
    /// The justfile the pre-scan located, if any.
    pub justfile: Option<PathBuf>,
}

/// just-next's engine-override flags.
///
/// These are not upstream flags, so they are stripped from argv before the V1
/// engine sees them.
pub const ENGINE_FLAGS: &[&str] = &["--legacy", "--next"];

/// Decide which engine handles this invocation.
///
/// `args` is the full argv, including the program name. This performs a
/// best-effort pre-scan: it honours `--justfile` and `--working-directory` well
/// enough to find the file, reads it, and runs [`detect`] over it.
///
/// Anything that does not resolve to a readable justfile routes to V1, which
/// then performs the authoritative search and produces upstream's own error
/// messages. That keeps `just --version`, `just --init`, shell completions and
/// every failure mode byte-identical to upstream.
pub fn route(args: &[impl AsRef<OsStr>]) -> Route {
    let args: Vec<&OsStr> = args.iter().map(AsRef::as_ref).collect();

    // Explicit overrides, checked before anything touches the filesystem.
    if args.iter().any(|arg| *arg == OsStr::new("--legacy")) {
        return Route { engine: Engine::V1, justfile: None };
    }

    let working_dir = flag_value(&args, &["-d", "--working-directory"])
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());

    let Some(working_dir) = working_dir else {
        return Route { engine: Engine::V1, justfile: None };
    };

    let Some(justfile) = locate(&args, &working_dir) else {
        // `just <folder>/<recipe>` looks only in `<folder>`, never in its
        // parents. If that folder exists but holds no justfile, falling back to
        // V1 would quietly run a parent's recipe instead; route to V2 so it
        // reports the empty directory.
        if folder_scoped_directory(&args, &working_dir).is_some() {
            return Route { engine: Engine::V2, justfile: None };
        }
        return Route { engine: Engine::V1, justfile: None };
    };

    let Ok(content) = std::fs::read_to_string(&justfile) else {
        return Route { engine: Engine::V1, justfile: None };
    };

    let forced_next = args.iter().any(|arg| *arg == OsStr::new("--next"));
    let engine = if forced_next { Engine::V2 } else { detect(&content) };

    Route { engine, justfile: Some(justfile) }
}

/// Find the justfile this command line refers to.
fn locate(args: &[&OsStr], working_dir: &Path) -> Option<PathBuf> {
    if let Some(path) = flag_value(args, &["-f", "--justfile"]) {
        // `--justfile -` reads from stdin, which cannot be read twice; leave it
        // to V1.
        if path == OsStr::new("-") {
            return None;
        }
        return Some(working_dir.join(path));
    }

    if let Some((dir, _)) = folder_scoped(args) {
        let dir = working_dir.join(dir);
        if dir.is_dir() {
            return search::justfile_in(&dir);
        }
    }

    search::justfile_from(working_dir)
}

/// The existing directory a folder-scoped invocation names, if it names one.
///
/// `None` when the command line is not folder-scoped, or names a path that is
/// not a directory — in which case it is an ordinary recipe name or a search
/// argument, and belongs to V1.
fn folder_scoped_directory(args: &[&OsStr], working_dir: &Path) -> Option<PathBuf> {
    // A `--justfile` argument takes precedence over folder scoping.
    if flag_value(args, &["-f", "--justfile"]).is_some() {
        return None;
    }

    let (dir, recipe) = folder_scoped(args)?;

    // A bare `sub/` is upstream's search-directory argument, not just-next's
    // folder-scoped form — `just --init sub/` names a directory that is
    // *expected* to have no justfile yet. Only `sub/recipe` claims the
    // stricter no-walk-up behaviour.
    if recipe.is_empty() {
        return None;
    }

    let dir = working_dir.join(dir);
    dir.is_dir().then_some(dir)
}

/// Split a leading `<folder>/<recipe>` argument, if the command line has one.
///
/// The recipe part is empty for a trailing slash (`<folder>/`), meaning "that
/// folder's default recipe".
pub fn folder_scoped(args: &[&OsStr]) -> Option<(String, String)> {
    let first = first_positional(args)?.to_str()?;
    let (dir, recipe) = split_folder_argument(first)?;
    Some((dir.to_string(), recipe.to_string()))
}

/// Split a `<folder>/<recipe>` argument into its directory and recipe parts.
///
/// Returns `None` when `argument` has no `/`, i.e. it is a plain recipe name.
/// The recipe part is empty for a trailing slash (`<folder>/`).
pub fn split_folder_argument(argument: &str) -> Option<(&str, &str)> {
    let (dir, recipe) = argument.rsplit_once('/')?;

    // A leading slash is an absolute path, not a folder/recipe pair.
    if dir.is_empty() {
        return None;
    }

    Some((dir, recipe))
}

/// Flags that consume the argument after them, for positional scanning.
const VALUE_FLAGS: &[&str] = &["-f", "--justfile", "-d", "--working-directory"];

/// The first argument that is not a flag or a flag's value.
fn first_positional<'a>(args: &[&'a OsStr]) -> Option<&'a OsStr> {
    let mut index = 1;

    while index < args.len() {
        let arg = args[index];
        let text = arg.to_str().unwrap_or("");

        if text == "--" {
            return args.get(index + 1).copied();
        }

        if text.starts_with('-') && text.len() > 1 {
            if VALUE_FLAGS.contains(&text) {
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        return Some(arg);
    }

    None
}

/// The value of the first of `names` present in `args`.
///
/// Handles both `--flag value` and `--flag=value`.
fn flag_value<'a>(args: &[&'a OsStr], names: &[&str]) -> Option<&'a OsStr> {
    let mut index = 1;

    while index < args.len() {
        let arg = args[index];
        let text = arg.to_str().unwrap_or("");

        if names.contains(&text) {
            return args.get(index + 1).copied();
        }

        for name in names {
            let prefix = format!("{name}=");
            if let Some(rest) = text.strip_prefix(prefix.as_str()) {
                return Some(OsStr::new(rest));
            }
        }

        index += 1;
    }

    None
}

/// Constructs that are legal in V1 and not in V2.
fn top_level_is_v1(line: &str) -> bool {
    // `foo := "bar"`, `export foo := "bar"`, `alias b := build`, `set shell := [..]`.
    // V2 spells all of these with `=`.
    if line.contains(":=") {
        return true;
    }

    // `{{ ... }}` interpolation. V2 uses shell `$VAR` instead.
    if line.contains("{{") {
        return true;
    }

    // Backtick command evaluation. V2 uses `$( ... )`.
    if line.contains('`') {
        return true;
    }

    // Attributes: `[private]`, `[group('x')]`, `[confirm]`, ...
    if line.starts_with('[') {
        return true;
    }

    // Statements with no V2 equivalent.
    for keyword in ["import ", "import?", "mod ", "mod?", "unexport "] {
        if line.starts_with(keyword) {
            return true;
        }
    }

    // Settings that exist only in V1.
    if let Some(rest) = line.strip_prefix("set ") {
        let name = rest
            .split(|c: char| c.is_whitespace() || c == '=')
            .next()
            .unwrap_or("");
        if V1_ONLY_SETTINGS.contains(&name) {
            return true;
        }
    }

    if let Some((signature, deps)) = split_recipe_header(line) {
        // Dependencies with arguments: `foo: (bar "baz")`.
        if deps.contains('(') {
            return true;
        }

        // Exported parameters: `wut $FOO='a':`. V1 exports the parameter into
        // the recipe's environment; V2 has no such syntax.
        if signature.contains('$') {
            return true;
        }
    }

    false
}

/// Settings understood only by upstream `just`.
///
/// Upstream's full `Setting` enum, minus the three both dialects share
/// (`export`, `positional-arguments`, `shell`). Keep in sync with
/// `crates/just-v1/src/setting.rs` when resyncing.
const V1_ONLY_SETTINGS: &[&str] = &[
    "allow-duplicate-recipes",
    "allow-duplicate-variables",
    "default-list",
    "default-script",
    "dotenv-command",
    "dotenv-filename",
    "dotenv-load",
    "dotenv-override",
    "dotenv-path",
    "dotenv-required",
    "fallback",
    "guards",
    "ignore-comments",
    "indentation",
    "lazy",
    "lists",
    "minimum-version",
    "no-cd",
    "no-exit-message",
    "quiet",
    "script-interpreter",
    "tempdir",
    "unstable",
    "windows-powershell",
    "windows-shell",
    "working-directory",
];

/// Constructs at the top level that are legal in V2 and not in V1.
fn top_level_is_v2(line: &str) -> bool {
    // `export NAME="value"` — V1 requires `export NAME := "value"`, and the
    // `:=` check above has already claimed that form for V1.
    if let Some(rest) = line.strip_prefix("export ") {
        if let Some((name, _)) = rest.split_once('=') {
            return is_identifier(name.trim());
        }
    }

    false
}

/// Constructs inside a recipe body that indicate V2.
///
/// Both dialects pass unrecognized body lines to a shell, so these are
/// judgement calls rather than parse errors under V1 — they signal *intent*.
fn body_is_v2(line: &str, params: &[String]) -> bool {
    let line = line.trim_start_matches(['@', '-']).trim_start();

    // `export CC=clang` in a body. Under V1 this is a shell export that is lost
    // at the end of the line, which is exactly the gotcha V2 fixes.
    if let Some(rest) = line.strip_prefix("export ") {
        if let Some((name, _)) = rest.split_once('=') {
            if is_identifier(name.trim()) {
                return true;
            }
        }
    }

    // `FOO=$(echo bar)` as a whole line — a V2 assignment that persists to
    // later lines. Under V1 it is a shell assignment discarded immediately.
    //
    // The whole line must be assignments and nothing else. `FOO=bar cmd` is a
    // command with an environment prefix, and `x=1 && echo $x` is a compound
    // shell command; both mean the same under either dialect, so neither is
    // evidence of anything.
    let (assignments, remainder) = split_leading_assignments(line);
    if !assignments.is_empty() && remainder.is_empty() {
        let compound = assignments.iter().any(|(_, value)| {
            value.contains("&&")
                || value.contains("||")
                || value.contains(';')
                || value.contains('|')
        });
        if !compound {
            return true;
        }
    }

    // `$PARAM` where PARAM is a parameter of the enclosing recipe. V1 exposes
    // parameters as `{{PARAM}}`, never as an environment variable, unless
    // `set export` is in play — and that spelling would have tripped `:=`.
    for param in params {
        if references_variable(line, param) {
            return true;
        }
    }

    false
}

/// Whether `line` contains a shell reference to `name`: `$name` or `${name}`.
fn references_variable(line: &str, name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;

    while let Some(offset) = line[i..].find('$') {
        let start = i + offset + 1;
        let rest = &line[start..];

        let candidate = rest.strip_prefix('{').unwrap_or(rest);
        if candidate.starts_with(name) {
            let after = &candidate[name.len()..];
            let terminated = after
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_');
            if terminated {
                // A `$$` is an escaped dollar or the shell's PID, not a variable.
                if start < 2 || bytes[start - 2] != b'$' {
                    return true;
                }
            }
        }

        i = start;
    }

    false
}

/// Split a recipe header into its `name + parameters` and `dependencies` parts.
///
/// Returns `None` when `line` is not a recipe header.
fn split_recipe_header(line: &str) -> Option<(&str, &str)> {
    // Statements can contain a colon in a quoted value, so rule them out first.
    for keyword in ["export ", "alias ", "set ", "import", "mod "] {
        if line.starts_with(keyword) {
            return None;
        }
    }

    let line = line.strip_prefix('@').unwrap_or(line);
    let (before, after) = line.split_once(':')?;

    let name = before.split_whitespace().next()?;
    if !is_identifier_with_dashes(name) {
        return None;
    }

    Some((before, after))
}

/// The parameter names of a recipe header, or `None` if `line` is not one.
fn recipe_header_params(line: &str) -> Option<Vec<String>> {
    let (before, _) = split_recipe_header(line)?;

    let params = before
        .split_whitespace()
        .skip(1)
        .map(|part| {
            let part = part.trim_start_matches(['*', '+', '$']);
            part.split_once('=').map_or(part, |(name, _)| name).to_string()
        })
        .filter(|name| is_identifier_with_dashes(name))
        .collect();

    Some(params)
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_identifier_with_dashes(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_engine(content: &str, expected: Engine) {
        assert_eq!(detect(content), expected, "justfile:\n{content}");
    }

    #[test]
    fn set_next_forces_v2() {
        // Even alongside V1 syntax, the explicit opt-in wins.
        assert_engine("set next\n\nfoo := \"bar\"\n\nbuild:\n    echo {{foo}}\n", Engine::V2);
    }

    #[test]
    fn colon_equals_is_v1() {
        assert_engine("foo := \"bar\"\n\nbuild:\n    echo {{foo}}\n", Engine::V1);
    }

    #[test]
    fn interpolation_is_v1() {
        assert_engine("build:\n    echo {{env_var(\"HOME\")}}\n", Engine::V1);
    }

    #[test]
    fn attributes_are_v1() {
        assert_engine("[private]\nbuild:\n    cargo build\n", Engine::V1);
    }

    #[test]
    fn backticks_are_v1() {
        assert_engine("build:\n    echo `date`\n", Engine::V1);
    }

    #[test]
    fn imports_and_modules_are_v1() {
        assert_engine("import 'other.just'\n\nbuild:\n    cargo build\n", Engine::V1);
        assert_engine("mod sub\n\nbuild:\n    cargo build\n", Engine::V1);
    }

    #[test]
    fn v1_only_settings_are_v1() {
        assert_engine("set dotenv-load\n\nbuild:\n    cargo build\n", Engine::V1);
        assert_engine("set windows-shell := ['pwsh']\n\nbuild:\n    true\n", Engine::V1);
    }

    #[test]
    fn dependency_arguments_are_v1() {
        assert_engine("build: (setup \"x\")\n    cargo build\n", Engine::V1);
    }

    #[test]
    fn shell_export_is_v2() {
        // The README's headline difference: `export NAME="value"`, no `:=`.
        assert_engine("export PATH=\"node_modules/.bin:$PATH\"\n\nbuild:\n    cargo build\n", Engine::V2);
    }

    #[test]
    fn recipe_body_assignment_is_v2() {
        assert_engine("build:\n    FOO=$(echo bar)\n    echo $FOO\n", Engine::V2);
    }

    #[test]
    fn several_assignments_on_one_line_are_v2() {
        assert_engine("build:\n    ONE=1 TWO=2\n    echo $ONE$TWO\n", Engine::V2);
    }

    #[test]
    fn an_environment_prefix_is_not_a_v2_signal() {
        // `FOO=bar cmd` scopes FOO to cmd under both dialects, so it says
        // nothing about which one the author meant.
        assert_engine("build:\n    FOO=bar cargo build\n", AMBIGUOUS_DEFAULT);
    }

    #[test]
    fn recipe_body_export_is_v2() {
        assert_engine("build:\n    export CC=clang\n    make\n", Engine::V2);
    }

    #[test]
    fn parameter_reference_is_v2() {
        assert_engine("run NAME *ARGS:\n    ./program $NAME $ARGS\n", Engine::V2);
    }

    #[test]
    fn v1_signals_beat_v2_signals() {
        // A legacy justfile whose recipe body happens to hold a shell assignment
        // stays on V1 — `:=` is decisive.
        assert_engine("foo := \"bar\"\n\nbuild:\n    FOO=1\n    echo {{foo}}\n", Engine::V1);
    }

    #[test]
    fn plain_justfile_is_ambiguous() {
        // Parses identically under both dialects, so the fallback decides.
        assert_engine("build:\n    cargo build\n", AMBIGUOUS_DEFAULT);
        assert_eq!(
            detect_with("build:\n    cargo build\n", Engine::V2),
            Engine::V2
        );
    }

    #[test]
    fn shell_conditionals_are_not_v1_signals() {
        // Regression: the old detector matched a bare `if `/`else `, sending any
        // recipe containing shell control flow to V1.
        assert_engine(
            "deploy TARGET:\n    if [ -z \"$TARGET\" ]; then\n      echo none\n    else\n      echo $TARGET\n    fi\n",
            Engine::V2,
        );
    }

    #[test]
    fn comments_do_not_signal() {
        assert_engine("# foo := \"bar\"\nbuild:\n    cargo build\n", AMBIGUOUS_DEFAULT);
    }

    #[test]
    fn shebang_body_is_not_misread() {
        assert_engine("build:\n    #!/bin/bash\n    echo hi\n", AMBIGUOUS_DEFAULT);
    }

    #[test]
    fn escaped_dollar_is_not_a_parameter_reference() {
        assert_engine("run NAME:\n    echo $$NAME\n", AMBIGUOUS_DEFAULT);
    }

    #[test]
    fn variable_reference_requires_exact_name() {
        assert!(references_variable("echo $FOO", "FOO"));
        assert!(references_variable("echo ${FOO}", "FOO"));
        assert!(references_variable("echo $FOO/bar", "FOO"));
        assert!(!references_variable("echo $FOOBAR", "FOO"));
        assert!(!references_variable("echo $BAR", "FOO"));
        assert!(!references_variable("echo FOO", "FOO"));
    }

    #[test]
    fn recipe_header_parameters_are_extracted() {
        assert_eq!(
            recipe_header_params("run NAME *ARGS:"),
            Some(vec!["NAME".to_string(), "ARGS".to_string()])
        );
        assert_eq!(
            recipe_header_params("deploy ENV=staging:"),
            Some(vec!["ENV".to_string()])
        );
        assert_eq!(recipe_header_params("build:"), Some(vec![]));
        assert_eq!(recipe_header_params("    echo hi"), None);
        assert_eq!(recipe_header_params("export FOO=bar"), None);
    }
}
