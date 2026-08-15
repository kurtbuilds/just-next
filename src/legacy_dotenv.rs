//! `.env` loading for justfiles that route to the V1 engine.
//!
//! The V2 engine loads a `.env` as part of its automatic environment setup. V1
//! is upstream `just`, which loads one only when told to — `set dotenv-load`,
//! `--dotenv-path`, and friends. Detection is per *file*, so a single `{{ }}`
//! or `:=` anywhere in a justfile sends the whole thing to V1, and every recipe
//! in it silently loses its `.env`. The failure lands far downstream, as an
//! empty variable in a command that has nothing to do with the construct that
//! caused it.
//!
//! So: before handing argv to V1, put the `.env` into the process environment,
//! which upstream passes through to recipes. Two rules keep this out of
//! upstream's way:
//!
//! * A justfile that configures dotenv itself, or an invocation that does, is
//!   left entirely alone — upstream owns the behaviour in that case, including
//!   `set dotenv-load := false`, which must keep meaning what it says.
//! * Variables already in the environment are never overwritten, matching both
//!   V2's rule and upstream's non-`dotenv-override` default.
//!
//! This is a deliberate divergence from upstream, whose default is to ignore a
//! `.env` entirely. See VENDORING.md.

use std::ffi::OsStr;
use std::path::Path;

use crate::v2::environment::{find_dotenv, read_dotenv};

/// Upstream flags that put dotenv handling under the invocation's control.
const DOTENV_FLAGS: &[&str] = &[
    "--no-dotenv",
    "--dotenv-command",
    "--dotenv-filename",
    "--dotenv-path",
];

/// Load the `.env` that applies to `justfile` into the process environment.
///
/// A no-op when the justfile or the command line configures dotenv itself, or
/// when there is no `.env` to load.
pub fn preload(justfile: &Path, args: &[impl AsRef<OsStr>]) {
    if args
        .iter()
        .any(|arg| flag_name(arg.as_ref()).is_some_and(|name| DOTENV_FLAGS.contains(&name)))
    {
        return;
    }

    let Ok(content) = std::fs::read_to_string(justfile) else {
        return;
    };
    if configures_dotenv(&content) {
        return;
    }

    let Some(dir) = justfile.parent() else {
        return;
    };
    let Some(dotenv) = find_dotenv(dir) else {
        return;
    };

    for (key, value) in read_dotenv(&dotenv) {
        if std::env::var_os(&key).is_none() {
            // SAFETY: single-threaded — this runs before V1 is handed argv, and
            // nothing else in the process has started a thread by this point.
            unsafe { std::env::set_var(&key, &value) };
        }
    }
}

/// Whether a justfile has a `set dotenv-…` line of its own.
fn configures_dotenv(content: &str) -> bool {
    content.lines().any(|line| {
        line.trim()
            .strip_prefix("set")
            .is_some_and(|rest| rest.trim_start().starts_with("dotenv-"))
    })
}

/// The flag name in `arg`, with any `=value` cut off. `None` for positionals.
fn flag_name(arg: &OsStr) -> Option<&str> {
    let arg = arg.to_str()?;
    if !arg.starts_with("--") {
        return None;
    }
    Some(arg.split_once('=').map_or(arg, |(name, _)| name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_settings_are_recognised() {
        assert!(configures_dotenv("set dotenv-load\n"));
        assert!(configures_dotenv("set dotenv-load := false\n"));
        assert!(configures_dotenv("  set dotenv-path := 'x'\n"));
        assert!(configures_dotenv("set dotenv-override := true\n"));

        assert!(!configures_dotenv("set export\n"));
        assert!(!configures_dotenv("set shell := ['bash', '-c']\n"));
        // A recipe body that happens to mention one.
        assert!(!configures_dotenv("build:\n    echo set dotenv-load\n"));
    }

    #[test]
    fn dotenv_flags_are_recognised() {
        assert_eq!(flag_name(OsStr::new("--no-dotenv")), Some("--no-dotenv"));
        assert_eq!(
            flag_name(OsStr::new("--dotenv-path=x")),
            Some("--dotenv-path")
        );
        assert_eq!(flag_name(OsStr::new("-f")), None);
        assert_eq!(flag_name(OsStr::new("recipe")), None);
    }
}
