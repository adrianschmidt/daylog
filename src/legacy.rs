//! Detection and migration helpers for users coming from the
//! pre-rename `daylog` layout.
//!
//! All functions take explicit paths so they can be unit-tested with
//! `tempfile`-rooted fakes; the thin shims that resolve real
//! `dirs::config_dir()` paths live in `config.rs` and `cli::migrate_cmd`.

use std::path::{Path, PathBuf};

/// Returns `Some(legacy_dir)` if a `daylog/` config directory exists at
/// `parent` (typically `dirs::config_dir()`), and the corresponding
/// `vitalog/` directory does NOT exist. Returns `None` otherwise.
pub fn legacy_config_dir(parent: &Path) -> Option<PathBuf> {
    let legacy = parent.join("daylog");
    let current = parent.join("vitalog");
    if legacy.is_dir() && !current.exists() {
        Some(legacy)
    } else {
        None
    }
}

/// Returns `Some(legacy_db)` if `.daylog.db` exists in `notes_dir` and
/// `.vitalog.db` does NOT. Returns `None` otherwise.
pub fn legacy_db_path(notes_dir: &Path) -> Option<PathBuf> {
    let legacy = notes_dir.join(".daylog.db");
    let current = notes_dir.join(".vitalog.db");
    if legacy.is_file() && !current.exists() {
        Some(legacy)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn legacy_config_dir_returns_none_when_neither_exists() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(legacy_config_dir(tmp.path()), None);
    }

    #[test]
    fn legacy_config_dir_returns_some_when_only_old_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("daylog")).unwrap();
        let got = legacy_config_dir(tmp.path()).unwrap();
        assert_eq!(got, tmp.path().join("daylog"));
    }

    #[test]
    fn legacy_config_dir_returns_none_when_both_exist() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("daylog")).unwrap();
        std::fs::create_dir(tmp.path().join("vitalog")).unwrap();
        assert_eq!(legacy_config_dir(tmp.path()), None);
    }

    #[test]
    fn legacy_db_path_returns_none_when_neither_exists() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(legacy_db_path(tmp.path()), None);
    }

    #[test]
    fn legacy_db_path_returns_some_when_only_old_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".daylog.db"), b"").unwrap();
        let got = legacy_db_path(tmp.path()).unwrap();
        assert_eq!(got, tmp.path().join(".daylog.db"));
    }

    #[test]
    fn legacy_db_path_returns_none_when_both_exist() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".daylog.db"), b"").unwrap();
        std::fs::write(tmp.path().join(".vitalog.db"), b"").unwrap();
        assert_eq!(legacy_db_path(tmp.path()), None);
    }
}
