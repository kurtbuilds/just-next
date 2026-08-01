//! Integration tests for the features just-next's README calls out as
//! differences from `just`.
//!
//! Each module here maps to a section of that README. The upstream suite in
//! `tests/integration` covers V1 compatibility; this one covers what V2 adds.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tempfile::TempDir;

const JUST: &str = env!("CARGO_BIN_EXE_just");

mod arguments;
mod detection;
mod environment;
mod exports;
mod folders;
mod recipes;
mod settings;

/// A justfile in a temporary directory, plus the invocation to run against it.
#[must_use]
pub struct Test {
    args: Vec<String>,
    current_dir: PathBuf,
    env: BTreeMap<String, String>,
    justfile: Option<String>,
    tempdir: TempDir,
}

impl Test {
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            current_dir: PathBuf::new(),
            env: BTreeMap::new(),
            justfile: None,
            tempdir: tempfile::tempdir().unwrap(),
        }
    }

    /// Set the justfile contents. Leading indentation is stripped, so tests can
    /// use an indented raw string.
    pub fn justfile(mut self, text: &str) -> Self {
        self.justfile = Some(unindent(text));
        self
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_owned());
        self
    }

    pub fn args<const N: usize>(mut self, args: [&str; N]) -> Self {
        for arg in args {
            self = self.arg(arg);
        }
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    /// Write an additional file, creating parent directories as needed.
    pub fn write(self, path: impl AsRef<Path>, contents: &str) -> Self {
        let path = self.tempdir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        self
    }

    /// Write a file and mark it executable, for putting fake binaries on `PATH`.
    pub fn write_executable(self, path: impl AsRef<Path>, contents: &str) -> Self {
        let full = self.tempdir.path().join(path.as_ref());
        let test = self.write(path.as_ref(), contents);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&full, fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = full;

        test
    }

    pub fn create_dir(self, path: impl AsRef<Path>) -> Self {
        fs::create_dir_all(self.tempdir.path().join(path)).unwrap();
        self
    }

    /// Run from a subdirectory of the temporary directory.
    pub fn current_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.current_dir = path.as_ref().to_path_buf();
        self
    }

    pub fn dir(&self) -> &Path {
        self.tempdir.path()
    }

    fn output(&self) -> Output {
        if let Some(justfile) = &self.justfile {
            fs::write(self.tempdir.path().join("justfile"), justfile).unwrap();
        }

        // No `--quiet`: upstream's suppresses recipe stdout as well as command
        // echoing, and stdout is what these tests assert on. Both engines echo
        // commands to stderr, which is left out of the assertions instead.
        let mut command = Command::new(JUST);
        command
            .current_dir(self.tempdir.path().join(&self.current_dir))
            .args(&self.args)
            // This repo sets CARGO_TARGET_DIR, and the child would inherit it
            // and resolve `target/debug` outside the test's directory.
            .env_remove("CARGO_TARGET_DIR")
            .envs(&self.env);

        command.output().unwrap()
    }

    /// Run, expecting success, and assert on stdout.
    #[track_caller]
    pub fn stdout(self, expected: &str) {
        let output = self.output();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "expected success, got {}\nstdout: {stdout}\nstderr: {stderr}",
            output.status,
        );
        assert_eq!(stdout, unindent(expected), "stderr: {stderr}");
    }

    /// Run, expecting failure, and assert stderr contains `needle`.
    #[track_caller]
    pub fn fails_with(self, needle: &str) {
        let output = self.output();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !output.status.success(),
            "expected failure, got success\nstdout: {stdout}\nstderr: {stderr}",
        );
        assert!(
            stderr.contains(needle),
            "stderr did not contain {needle:?}\nstderr: {stderr}",
        );
    }

    /// Run, expecting success, and return stdout verbatim.
    #[track_caller]
    pub fn stdout_raw(self) -> String {
        let output = self.output();
        assert!(
            output.status.success(),
            "expected success, got {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

/// Strip the common leading indentation from every line, so justfiles can be
/// written inline without fighting Rust's indentation.
fn unindent(text: &str) -> String {
    let text = text.strip_prefix('\n').unwrap_or(text);

    let indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut result = String::new();
    for line in text.lines() {
        if line.len() >= indent {
            result.push_str(&line[indent..]);
        } else {
            result.push_str(line.trim_start());
        }
        result.push('\n');
    }
    result
}

#[test]
fn unindent_strips_common_leading_whitespace() {
    assert_eq!(unindent("\n  foo:\n    bar\n"), "foo:\n  bar\n");
    assert_eq!(unindent("foo\n"), "foo\n");
    assert_eq!(unindent("\n    a\n\n    b\n"), "a\n\nb\n");
}
