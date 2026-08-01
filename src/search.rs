//! Locating a justfile on disk.
//!
//! Used by [`crate::dispatch`] to read a justfile before deciding which engine
//! parses it, and by the V2 path to find the file it runs.
//!
//! This deliberately mirrors upstream's search (`justfile` and `.justfile`,
//! matched case-insensitively, walking up the directory tree) but does not have
//! to be exact: when the pre-scan fails to find a file, dispatch falls back to
//! V1, which then performs the authoritative search itself.

use std::path::{Path, PathBuf};

/// Justfile names, in search order, as upstream spells them.
pub const JUSTFILE_NAMES: &[&str] = &["justfile", ".justfile"];

/// Whether `name` is a justfile name, ignoring case.
fn is_justfile_name(name: &str) -> bool {
    JUSTFILE_NAMES
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

/// Find a justfile directly inside `dir`, without walking up the tree.
pub fn justfile_in(dir: &Path) -> Option<PathBuf> {
    // Read the directory so that `Justfile` and `JUSTFILE` are found on
    // case-sensitive filesystems too.
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| is_justfile_name(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();

    entries.sort();
    entries.into_iter().find(|path| path.is_file())
}

/// Find a justfile in `start` or any of its ancestors.
pub fn justfile_from(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(dir) = current {
        if let Some(found) = justfile_in(dir) {
            return Some(found);
        }
        current = dir.parent();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_justfile_in_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("justfile"), "build:\n    true\n").unwrap();

        assert_eq!(
            justfile_in(dir.path()),
            Some(dir.path().join("justfile"))
        );
    }

    #[test]
    fn does_not_walk_up_from_justfile_in() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("justfile"), "build:\n    true\n").unwrap();

        assert_eq!(justfile_in(&nested), None);
        assert_eq!(justfile_from(&nested), Some(dir.path().join("justfile")));
    }

    #[test]
    fn returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(justfile_in(dir.path()), None);
    }

    #[test]
    fn recognizes_case_variants() {
        assert!(is_justfile_name("justfile"));
        assert!(is_justfile_name("Justfile"));
        assert!(is_justfile_name("JUSTFILE"));
        assert!(is_justfile_name(".justfile"));
        assert!(!is_justfile_name("justfile.md"));
        assert!(!is_justfile_name("notajustfile"));
    }
}
