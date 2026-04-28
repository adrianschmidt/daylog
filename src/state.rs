//! Pending CLI state stored in `{notes_dir}/.daylog-state.toml`.
//!
//! This sidecar holds short-lived state that doesn't belong in the daily
//! notes themselves — currently only the pending bedtime between
//! `daylog sleep-start` and `daylog sleep-end`.
//!
//! ## Concurrency / crash semantics
//!
//! - **Atomicity:** [`save`] writes to a PID-suffixed temp file and renames
//!   into place, so a crash mid-write cannot leave the sidecar partially
//!   written. The temp filename includes [`std::process::id`] so concurrent
//!   processes don't collide on the temp path.
//! - **No advisory locking.** Two `daylog` processes performing
//!   read-modify-write on the sidecar can race: P1 reads, P2 reads, P1
//!   saves, P2 saves — P2 wins. For two `sleep-start` invocations this is
//!   the intended "last bedtime wins" semantic. For mixed `sleep-start` /
//!   `sleep-end` interleavings the outcome is best-effort.
//! - **Sleep-end ordering:** `cmd_sleep_end` writes the markdown note
//!   atomically *first*, then clears the sidecar. A crash between those
//!   two steps leaves the note written and the sidecar still pointing at
//!   the now-consumed bedtime, so the next `sleep-end` would treat it as
//!   live. The sidecar clear is therefore best-effort — a stale entry is
//!   bounded by the 24h staleness guard in `cmd_sleep_end`.
//! - **Corrupt sidecar:** [`load`] degrades to an empty state with a
//!   warning rather than failing, since blocking sleep logging on a
//!   corrupted sidecar would be worse UX than losing the in-flight bedtime.
//! - **Co-located with the DB by design:** the sidecar lives next to
//!   `.daylog.db` in `notes_dir`. This means sleep state is per-notes-dir,
//!   matching the data model. Users syncing `notes_dir` across machines
//!   should add `.daylog-state.toml` to their ignore list.

use chrono::{DateTime, Local, NaiveTime};
use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STATE_FILENAME: &str = ".daylog-state.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PendingState {
    #[serde(default)]
    pub sleep_start: Option<PendingSleepStart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingSleepStart {
    pub bedtime: NaiveTime,
    pub recorded_at: DateTime<Local>,
}

pub fn state_path(notes_dir: &Path) -> PathBuf {
    notes_dir.join(STATE_FILENAME)
}

/// Load pending state from `{notes_dir}/.daylog-state.toml`.
/// Returns empty state if the file is missing OR cannot be parsed
/// (warns on stderr in the latter case). Sleep state is recoverable —
/// failing here would block the user from logging.
pub fn load(notes_dir: &Path) -> PendingState {
    let path = state_path(notes_dir);
    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PendingState::default(),
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", path.display());
            return PendingState::default();
        }
    };
    match toml::from_str(&contents) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: {} is malformed ({e}), treating as empty. \
                 Delete the file or fix the TOML by hand to restore pending state.",
                path.display()
            );
            PendingState::default()
        }
    }
}

/// Save pending state atomically.
pub fn save(notes_dir: &Path, state: &PendingState) -> Result<()> {
    let path = state_path(notes_dir);
    let contents = toml::to_string(state).wrap_err("Failed to serialize pending state to TOML")?;
    let dir = path
        .parent()
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid state path: {}", path.display()))?;
    let temp = dir.join(format!(".daylog-state.tmp-{}", std::process::id()));
    std::fs::write(&temp, contents)
        .wrap_err_with(|| format!("Failed to write {}", temp.display()))?;
    std::fs::rename(&temp, &path)
        .wrap_err_with(|| format!("Failed to rename to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> PendingState {
        PendingState {
            sleep_start: Some(PendingSleepStart {
                bedtime: NaiveTime::from_hms_opt(22, 30, 0).unwrap(),
                recorded_at: Local::now(),
            }),
        }
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = load(dir.path());
        assert_eq!(s, PendingState::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = sample_state();
        save(dir.path(), &s).unwrap();
        let loaded = load(dir.path());
        assert_eq!(
            loaded.sleep_start.as_ref().unwrap().bedtime,
            s.sleep_start.as_ref().unwrap().bedtime
        );
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(state_path(dir.path()), "this is not toml{{{").unwrap();
        let s = load(dir.path());
        assert_eq!(s, PendingState::default());
    }

    #[test]
    fn save_clears_sleep_start_when_none() {
        let dir = tempfile::TempDir::new().unwrap();
        save(dir.path(), &sample_state()).unwrap();
        save(dir.path(), &PendingState::default()).unwrap();
        let loaded = load(dir.path());
        assert!(loaded.sleep_start.is_none());
    }

    #[test]
    fn save_does_not_leave_temp_file() {
        let dir = tempfile::TempDir::new().unwrap();
        save(dir.path(), &sample_state()).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        for name in &entries {
            assert!(
                !name.contains("tmp"),
                "leftover temp file: {name} (entries: {entries:?})"
            );
        }
    }
}
