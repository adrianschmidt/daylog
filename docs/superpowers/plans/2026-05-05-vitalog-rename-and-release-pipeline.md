# vitalog rename + automated release pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the fork from `daylog` to `vitalog` end-to-end (binary, package, repo, config dir, database filename, all docs), add an explicit `vitalog migrate` command for users with legacy paths, and stand up a semantic-release-driven CI pipeline that publishes versioned binaries to GitHub Releases.

**Architecture:** Two cleanly separable concerns — the rename (mechanical edits + a `legacy.rs` module exposing detection and migration helpers) and the release pipeline (semantic-release in a gated four-job workflow, authenticated via the `vitalog-release-bot` GitHub App). The rename keeps the existing TUI/data-flow architecture untouched; only paths, names, and string literals change.

**Tech Stack:** Rust (existing), `tempfile` (existing dev-dep, used for migration tests), semantic-release via `npx` (no `package.json` required), `actions/create-github-app-token` for App-token minting in CI.

**Spec:** `docs/superpowers/specs/2026-05-05-vitalog-rename-and-release-pipeline-design.md`

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `Cargo.toml` | modify | Rename crate (`name`, `repository`, `homepage`); drop `"tapes/"` from `exclude` |
| `LICENSE` | modify | Append vitalog modifications copyright line |
| `README.md` | modify | Rename throughout, add fork attribution, drop demo GIF line |
| `CLAUDE.md` (this repo) | modify | Rename throughout |
| `AGENTS.md`, `CONTRIBUTING.md`, `justfile` | modify | Rename throughout |
| `../CLAUDE.md` (workspace, untracked) | modify | PR-target update |
| `src/legacy.rs` | new | Detection + migration helpers (path-injectable, unit-testable) |
| `src/lib.rs` | modify | Add `pub mod legacy;` |
| `src/cli/mod.rs` | modify | `name = "vitalog"`; add `Migrate` variant + `pub mod migrate_cmd;` |
| `src/cli/migrate_cmd.rs` | new | `vitalog migrate` CLI entry — calls `legacy.rs`, prints summary |
| `src/main.rs` | modify | Replace `use daylog::…` with `use vitalog::…`; rename strings; dispatch `Migrate` |
| `src/config.rs` | modify | `~/.config/vitalog/`, `.vitalog.db`; on load, fall back to legacy paths via `legacy.rs` and print one-line stderr hint |
| `src/db.rs`, `src/state.rs`, `src/app.rs`, `src/frontmatter.rs`, `src/cli/completions.rs`, `src/cli/readme_cmd.rs`, `src/materializer/daily.rs`, `tests/integration.rs`, `tests/today.rs`, all other `src/cli/*.rs` | modify | Rename string literals (`daylog` → `vitalog` in error messages, comments, file paths) |
| `tests/migrate.rs` | new | End-to-end integration test for `vitalog migrate` |
| `scripts/bump-cargo.sh` | new | Cargo.toml + Cargo.lock version bump helper used by both the `prepare` job and `@semantic-release/exec` |
| `release.config.js` | new | semantic-release plugin config |
| `.github/workflows/release.yml` | replace | Push-to-main pipeline (analyze → prepare → build × 4 → publish), replacing the existing tag-triggered workflow |
| `tapes/` | delete | Demo recording shows `daylog` and is not being re-recorded |

---

## Task 1: Rename crate identity

**Files:**
- Modify: `Cargo.toml` (lines 2, 7, 8)
- Modify: `src/cli/mod.rs:15` (clap `name`)
- Modify: `src/main.rs` (all `use daylog::…` and `daylog::…` qualified calls)
- Modify: `src/app.rs:112,281` (UI strings)
- Modify: `src/cli/completions.rs:8` (clap_complete bin name)

Single atomic commit — Cargo.toml's `name` change forces the import-path renames; they must land together to keep `cargo check` clean.

- [ ] **Step 1: Edit `Cargo.toml`**

```toml
[package]
name = "vitalog"
version = "0.1.0"
edition = "2021"
description = "A terminal dashboard that tracks your life from markdown notes"
license = "MIT"
repository = "https://github.com/adrianschmidt/vitalog"
homepage = "https://github.com/adrianschmidt/vitalog"
```

(All other Cargo.toml fields stay untouched in this task. The `exclude` cleanup happens in Task 13.)

- [ ] **Step 2: Rename clap CLI name in `src/cli/mod.rs:15`**

```rust
#[command(
    name = "vitalog",
    version,
    about = "A terminal dashboard that tracks your life from markdown notes"
)]
```

- [ ] **Step 3: Replace all `use daylog::…` in `src/main.rs` with `use vitalog::…`**

Affects (per current grep) lines 5–8 and qualified calls on lines 23, 27, 64, 69, 141, 154, 169, 205, 209, 234, 251, 294, 303, 322, 346, 361, 376. The substitution is purely textual: every `daylog::` in this file becomes `vitalog::`.

- [ ] **Step 4: Update string literals in `src/app.rs`**

Line 112:
```rust
.block(Block::default().borders(Borders::ALL).title(" vitalog "))
```

Line 281:
```rust
Some(" Module change detected. Restart vitalog to apply.".to_string());
```

- [ ] **Step 5: Update bin name in `src/cli/completions.rs:8`**

```rust
clap_complete::generate(shell, &mut cmd, "vitalog", &mut std::io::stdout());
```

- [ ] **Step 6: Verify the crate still compiles**

```bash
cargo check
```

Expected: clean compile, no warnings about unresolved imports.

- [ ] **Step 7: Run the existing test suite**

```bash
cargo test
```

Expected: all existing tests pass. (Some test fixture strings still say "daylog" — that's Task 2's scope; tests should still pass since paths haven't changed yet.)

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/cli/mod.rs src/main.rs src/app.rs src/cli/completions.rs
git commit -m "refactor: rename crate from daylog to vitalog"
```

---

## Task 2: Rename internal path literals

**Files:**
- Modify: `src/config.rs` (lines 131, 165, 202, 217, plus on-load logic deferred to Task 6)
- Modify: `src/state.rs` (lines 1, 5, 13, 28, 30, 37, 55, 89)
- Modify: `src/db.rs:113`
- Modify: `src/frontmatter.rs:214`
- Modify: `src/main.rs:88,94,99,162,179,204,205` (hard-coded paths and error messages)
- Modify: `src/cli/readme_cmd.rs` (header comment, test name)
- Modify: `src/materializer/daily.rs:804,848` (test fixtures)
- Modify: `tests/integration.rs`, `tests/today.rs` (any `daylog` literals)

The default `notes_dir` in `vitalog init` becomes `~/vitalog-notes/` for new users; this does **not** rename existing users' configured `notes_dir`.

- [ ] **Step 1: In `src/config.rs:202`, change config dir name**

```rust
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?
        .join("vitalog");
    Ok(dir)
}
```

- [ ] **Step 2: In `src/config.rs:217`, change default DB filename**

```rust
None => self.notes_dir_path().join(".vitalog.db"),
```

- [ ] **Step 3: Update error messages in `src/config.rs:131,165`**

Line 131:
```rust
"Config not found at {}. Run `vitalog init` to create one.",
```

Line 165:
```rust
"Notes directory does not exist: {}. Check notes_dir in your config or run `vitalog init`.",
```

- [ ] **Step 4: Update `src/state.rs` constants and docstrings**

Line 37:
```rust
const STATE_FILENAME: &str = ".vitalog-state.toml";
```

Replace every `daylog` in the file's `//!` and `///` doc comments with `vitalog` (lines 1, 5, 13, 28, 30, 55).

Line 89 (tmp filename):
```rust
let temp = dir.join(format!(".vitalog-state.tmp-{}", std::process::id()));
```

- [ ] **Step 5: Update `src/db.rs:113` error message**

```rust
"Failed to open database at {}. Is another vitalog instance running?",
```

- [ ] **Step 6: Update `src/frontmatter.rs:214` tmp filename**

```rust
let temp_path = dir.join(format!(".vitalog-tmp-{}", std::process::id()));
```

- [ ] **Step 7: Update `src/main.rs` strings**

Lines 88, 94, 99 (init prompt + default):
```rust
print!("Notes directory [~/vitalog-notes/]: ");
…
"~/vitalog-notes".to_string()
…
"~/vitalog-notes".to_string()
```

Line 162:
```rust
eprintln!("Run `vitalog` to start!");
```

Line 179:
```rust
"Database not found at {}. Run `vitalog init` or `vitalog sync` first.",
```

Line 204 (comment): `daylog sleep-start` and `daylog sleep-end` → `vitalog sleep-start` and `vitalog sleep-end`.

- [ ] **Step 8: Update `src/cli/readme_cmd.rs` header comments and test**

Lines 1, 4 (`//!` doc):
```rust
//! `vitalog readme` — print the embedded README.md to stdout.
//!
//! README.md ships embedded via `include_str!`, so the
//! installed binary (`~/.cargo/bin/vitalog`) ships with its own docs —
```

Lines 24, 26 (test name and assertion):
```rust
fn readme_mentions_vitalog_food() {
    assert!(
        README.contains("vitalog food"),
```

- [ ] **Step 9: Update test fixtures in `src/materializer/daily.rs:804,848`**

```rust
let db_path = notes_dir.join(".vitalog.db");
```
(Both sites.)

- [ ] **Step 10: Update `src/cli/mod.rs:69` docstring**

The `SleepStart` variant's doc comment mentions `.daylog-state.toml`; change to `.vitalog-state.toml`.

- [ ] **Step 11: Sweep `tests/integration.rs` and `tests/today.rs` for any remaining `daylog` literals**

```bash
grep -n daylog tests/
```

Replace each occurrence in test code. The README content embedded via `include_str!` will be updated in Task 8; tests asserting on README content should be updated then, not now.

- [ ] **Step 12: Run the full test suite**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all green.

- [ ] **Step 13: Commit**

```bash
git add -u
git commit -m "refactor: rename internal paths and string literals to vitalog"
```

---

## Task 3: Add `legacy.rs` — detection helpers (TDD)

**Files:**
- Create: `src/legacy.rs`
- Modify: `src/lib.rs`

The module exposes path-injectable functions so tests use `tempfile`-rooted fakes rather than touching real `dirs::config_dir()`.

- [ ] **Step 1: Create `src/legacy.rs` with the module skeleton**

```rust
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
}
```

- [ ] **Step 2: Add `pub mod legacy;` to `src/lib.rs`**

Insert in alphabetical position relative to existing modules.

- [ ] **Step 3: Run the stub test**

```bash
cargo test --lib legacy
```

Expected: 1 test passes.

- [ ] **Step 4: Add failing test — legacy config dir, no vitalog yet**

Append inside `mod tests`:

```rust
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
```

- [ ] **Step 5: Run the tests**

```bash
cargo test --lib legacy
```

Expected: all 6 pass (the implementation is already in place from Step 1).

- [ ] **Step 6: Commit**

```bash
git add src/legacy.rs src/lib.rs
git commit -m "feat(legacy): add detection helpers for daylog-era paths"
```

---

## Task 4: `legacy.rs` — config dir migration (TDD)

**Files:**
- Modify: `src/legacy.rs`

- [ ] **Step 1: Write a failing test for the happy path**

Append inside `mod tests`:

```rust
    #[test]
    fn migrate_config_dir_moves_dir() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join("daylog");
        let to = tmp.path().join("vitalog");
        std::fs::create_dir(&from).unwrap();
        std::fs::write(from.join("config.toml"), b"hello").unwrap();

        let moved = migrate_config_dir(&from, &to).unwrap();

        assert!(moved);
        assert!(!from.exists());
        assert!(to.is_dir());
        assert_eq!(std::fs::read(to.join("config.toml")).unwrap(), b"hello");
    }
```

- [ ] **Step 2: Run to confirm it fails to compile**

```bash
cargo test --lib legacy
```

Expected: compile error — `migrate_config_dir` not defined.

- [ ] **Step 3: Implement `migrate_config_dir`**

Add to `src/legacy.rs` (above `mod tests`):

```rust
use color_eyre::Result;
use color_eyre::eyre::{eyre, WrapErr};

/// Moves `from` → `to` atomically (single rename syscall when on the
/// same filesystem). Returns `Ok(true)` when a move happened, `Ok(false)`
/// when nothing needed to be done (idempotent). Errors when both `from`
/// and `to` exist (refuses to overwrite).
pub fn migrate_config_dir(from: &Path, to: &Path) -> Result<bool> {
    match (from.exists(), to.exists()) {
        (false, _) => Ok(false),
        (true, true) => Err(eyre!(
            "Both legacy ({}) and current ({}) config directories exist; \
             refusing to overwrite. Resolve manually.",
            from.display(),
            to.display(),
        )),
        (true, false) => {
            std::fs::rename(from, to)
                .wrap_err_with(|| format!("rename {} → {}", from.display(), to.display()))?;
            Ok(true)
        }
    }
}
```

- [ ] **Step 4: Run the test**

```bash
cargo test --lib legacy::tests::migrate_config_dir_moves_dir
```

Expected: PASS.

- [ ] **Step 5: Add idempotency test**

```rust
    #[test]
    fn migrate_config_dir_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join("daylog");
        let to = tmp.path().join("vitalog");

        // No-op when neither side exists.
        assert!(!migrate_config_dir(&from, &to).unwrap());

        // No-op when only the destination exists.
        std::fs::create_dir(&to).unwrap();
        assert!(!migrate_config_dir(&from, &to).unwrap());
    }
```

- [ ] **Step 6: Add refuse-to-overwrite test**

```rust
    #[test]
    fn migrate_config_dir_refuses_overwrite() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join("daylog");
        let to = tmp.path().join("vitalog");
        std::fs::create_dir(&from).unwrap();
        std::fs::create_dir(&to).unwrap();

        let err = migrate_config_dir(&from, &to).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
        assert!(from.exists() && to.exists());
    }
```

- [ ] **Step 7: Run all legacy tests**

```bash
cargo test --lib legacy
```

Expected: 9 passing (6 from Task 3 + 3 new).

- [ ] **Step 8: Commit**

```bash
git add src/legacy.rs
git commit -m "feat(legacy): add migrate_config_dir with idempotency + refuse-overwrite"
```

---

## Task 5: `legacy.rs` — DB and sidecar migration (TDD)

**Files:**
- Modify: `src/legacy.rs`

- [ ] **Step 1: Write a failing test for happy-path DB rename**

```rust
    #[test]
    fn migrate_db_renames_main_file() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join(".daylog.db");
        let to = tmp.path().join(".vitalog.db");
        std::fs::write(&from, b"sqlite content").unwrap();

        let moved = migrate_db(&from, &to).unwrap();

        assert!(moved);
        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"sqlite content");
    }
```

- [ ] **Step 2: Run to confirm compile failure**

```bash
cargo test --lib legacy
```

Expected: `migrate_db` not defined.

- [ ] **Step 3: Implement `migrate_db` (file only, sidecars next)**

Append to `src/legacy.rs` (above `mod tests`):

```rust
/// Moves a `.daylog.db` file to `.vitalog.db`. Also moves the
/// SQLite WAL/SHM sidecar files (`.db-wal`, `.db-shm`) when present,
/// matching the main file's behavior. Returns `Ok(true)` if the main
/// file moved, `Ok(false)` if no-op. Errors when both `from` and `to`
/// exist (refuses to overwrite).
pub fn migrate_db(from: &Path, to: &Path) -> Result<bool> {
    match (from.is_file(), to.exists()) {
        (false, _) => Ok(false),
        (true, true) => Err(eyre!(
            "Both legacy ({}) and current ({}) database files exist; \
             refusing to overwrite. Resolve manually.",
            from.display(),
            to.display(),
        )),
        (true, false) => {
            std::fs::rename(from, to)
                .wrap_err_with(|| format!("rename {} → {}", from.display(), to.display()))?;
            for suffix in ["-wal", "-shm"] {
                let s_from = with_suffix(from, suffix);
                let s_to = with_suffix(to, suffix);
                if s_from.is_file() && !s_to.exists() {
                    std::fs::rename(&s_from, &s_to).wrap_err_with(|| {
                        format!("rename {} → {}", s_from.display(), s_to.display())
                    })?;
                }
            }
            Ok(true)
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}
```

- [ ] **Step 4: Run test**

```bash
cargo test --lib legacy::tests::migrate_db_renames_main_file
```

Expected: PASS.

- [ ] **Step 5: Add sidecar tests**

```rust
    #[test]
    fn migrate_db_renames_sidecars_when_present() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join(".daylog.db");
        let to = tmp.path().join(".vitalog.db");
        std::fs::write(&from, b"main").unwrap();
        std::fs::write(tmp.path().join(".daylog.db-wal"), b"wal").unwrap();
        std::fs::write(tmp.path().join(".daylog.db-shm"), b"shm").unwrap();

        assert!(migrate_db(&from, &to).unwrap());

        assert!(!tmp.path().join(".daylog.db-wal").exists());
        assert!(!tmp.path().join(".daylog.db-shm").exists());
        assert_eq!(std::fs::read(tmp.path().join(".vitalog.db-wal")).unwrap(), b"wal");
        assert_eq!(std::fs::read(tmp.path().join(".vitalog.db-shm")).unwrap(), b"shm");
    }

    #[test]
    fn migrate_db_skips_missing_sidecars() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join(".daylog.db");
        let to = tmp.path().join(".vitalog.db");
        std::fs::write(&from, b"main").unwrap();
        // No sidecars created.

        assert!(migrate_db(&from, &to).unwrap());
        assert!(to.is_file());
    }

    #[test]
    fn migrate_db_refuses_overwrite() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join(".daylog.db");
        let to = tmp.path().join(".vitalog.db");
        std::fs::write(&from, b"main").unwrap();
        std::fs::write(&to, b"existing").unwrap();

        let err = migrate_db(&from, &to).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn migrate_db_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let from = tmp.path().join(".daylog.db");
        let to = tmp.path().join(".vitalog.db");

        assert!(!migrate_db(&from, &to).unwrap());
        std::fs::write(&to, b"x").unwrap();
        assert!(!migrate_db(&from, &to).unwrap());
    }
```

- [ ] **Step 6: Run all legacy tests**

```bash
cargo test --lib legacy
```

Expected: 13 passing.

- [ ] **Step 7: Commit**

```bash
git add src/legacy.rs
git commit -m "feat(legacy): add migrate_db with sidecar handling + refuse-overwrite"
```

---

## Task 6: Auto-fallback in config load + stderr hint

When `Config::load()` is called, if the new `~/.config/vitalog/config.toml` does not exist but the legacy `~/.config/daylog/config.toml` does, fall back to reading the legacy file and print a one-line stderr hint pointing at `vitalog migrate`. Same for the DB path resolved by `db_path()`.

The hint must print **at most once per process invocation** (using a `OnceLock` or similar guard) so that a single `vitalog status` invocation that touches both config and DB doesn't print twice.

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write a failing integration-style test in `src/config.rs`**

Append to the end of `src/config.rs` (or in `tests/` if there's no inline test module):

```rust
#[cfg(test)]
mod legacy_fallback_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn config_path_falls_back_to_legacy_when_only_old_exists() {
        let tmp = TempDir::new().unwrap();
        let parent = tmp.path();
        std::fs::create_dir(parent.join("daylog")).unwrap();
        std::fs::write(parent.join("daylog/config.toml"), "notes_dir = \"~/x\"\n").unwrap();

        let resolved = resolve_config_path(parent);

        assert_eq!(resolved, parent.join("daylog/config.toml"));
    }

    #[test]
    fn config_path_uses_current_when_both_exist() {
        let tmp = TempDir::new().unwrap();
        let parent = tmp.path();
        std::fs::create_dir(parent.join("daylog")).unwrap();
        std::fs::create_dir(parent.join("vitalog")).unwrap();
        std::fs::write(parent.join("vitalog/config.toml"), "").unwrap();

        let resolved = resolve_config_path(parent);

        assert_eq!(resolved, parent.join("vitalog/config.toml"));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib resolve_config_path
```

Expected: `resolve_config_path` not defined.

- [ ] **Step 3: Add `resolve_config_path` and refactor `Config::config_path`**

In `src/config.rs`, add:

```rust
/// Path-injectable variant of `Config::config_path()`. Prefers
/// `<parent>/vitalog/config.toml`; falls back to
/// `<parent>/daylog/config.toml` when the new path does not exist but
/// the legacy one does. Pure — no I/O side effects, no logging.
pub(crate) fn resolve_config_path(parent: &Path) -> PathBuf {
    let current = parent.join("vitalog").join("config.toml");
    if current.exists() {
        return current;
    }
    let legacy = parent.join("daylog").join("config.toml");
    if legacy.exists() {
        return legacy;
    }
    current  // best default for "doesn't exist anywhere yet" — vitalog wins
}
```

Modify `Config::config_path()` to delegate:

```rust
pub fn config_path() -> Result<PathBuf> {
    let parent = dirs::config_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?;
    Ok(resolve_config_path(&parent))
}
```

`Config::config_dir()` keeps returning the *new* `vitalog/` directory (it's used by the init command to decide where to write).

- [ ] **Step 4: Run the new tests**

```bash
cargo test --lib resolve_config_path
```

Expected: 2 PASS.

- [ ] **Step 5: Add the same pattern for `db_path()`**

In `Config::db_path()`, after computing the would-be `.vitalog.db` path, fall back to `.daylog.db` when the vitalog file doesn't exist but the legacy one does:

```rust
pub fn db_path(&self) -> PathBuf {
    if let Some(p) = &self.db_path {
        return expand_tilde(p);
    }
    let notes = self.notes_dir_path();
    let current = notes.join(".vitalog.db");
    if current.exists() {
        return current;
    }
    let legacy = notes.join(".daylog.db");
    if legacy.is_file() {
        return legacy;
    }
    current
}
```

- [ ] **Step 6: Add the stderr hint**

Add a `OnceLock<()>` guard at module scope:

```rust
use std::sync::OnceLock;

static LEGACY_HINT_PRINTED: OnceLock<()> = OnceLock::new();

fn print_legacy_hint_once() {
    if LEGACY_HINT_PRINTED.set(()).is_ok() {
        eprintln!(
            "Note: Found legacy daylog config at ~/.config/daylog/.\n\
             Run `vitalog migrate` to move it to ~/.config/vitalog/."
        );
    }
}
```

Call `print_legacy_hint_once()` from `Config::config_path()` after deciding to use the legacy path, and from `db_path()` similarly when falling back. Each callsite is a one-line addition immediately before the `return legacy` (or equivalent).

- [ ] **Step 7: Add a test that the hint prints once**

This is hard to test cleanly without capturing stderr. Acceptable: skip the unit test for the OnceLock and verify manually. Add a brief `// Manual smoke: run twice in one process and ensure single message` comment near `print_legacy_hint_once`.

- [ ] **Step 8: Run the full test suite**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: green.

- [ ] **Step 9: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): fall back to legacy daylog paths with one-time stderr hint"
```

---

## Task 7: `vitalog migrate` CLI command

**Files:**
- Create: `src/cli/migrate_cmd.rs`
- Create: `tests/migrate.rs`
- Modify: `src/cli/mod.rs` (add `pub mod migrate_cmd;` and `Migrate` variant)
- Modify: `src/main.rs` (dispatch `Commands::Migrate`)

- [ ] **Step 1: Create `src/cli/migrate_cmd.rs`**

```rust
//! `vitalog migrate` — move legacy daylog config dir + database file
//! to the vitalog locations. Idempotent; refuses to overwrite when
//! both old and new locations contain data.

use color_eyre::Result;
use std::path::PathBuf;

use crate::config::Config;
use crate::legacy;

pub fn execute() -> Result<()> {
    let parent = dirs::config_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?;
    let legacy_cfg = parent.join("daylog");
    let current_cfg = parent.join("vitalog");

    let cfg_moved = legacy::migrate_config_dir(&legacy_cfg, &current_cfg)?;

    // After the config dir move, Config::load() resolves to the (now
    // vitalog) config and we can read notes_dir to find the database.
    // If neither old nor new config existed in the first place, load()
    // returns its usual "Config not found" error — propagate it.
    let db_moved = match Config::load() {
        Ok(cfg) => {
            let notes = cfg.notes_dir_path();
            let from = notes.join(".daylog.db");
            let to = notes.join(".vitalog.db");
            legacy::migrate_db(&from, &to)?
        }
        Err(_) if !cfg_moved => false,  // no config anywhere → nothing to migrate
        Err(e) => return Err(e),         // config existed but failed to load
    };

    print_summary(cfg_moved, &legacy_cfg, &current_cfg, db_moved);
    Ok(())
}

fn print_summary(cfg_moved: bool, from: &PathBuf, to: &PathBuf, db_moved: bool) {
    if !cfg_moved && !db_moved {
        println!("Nothing to do — already migrated (or no legacy paths found).");
        return;
    }
    if cfg_moved {
        println!("Moved config dir: {} → {}", from.display(), to.display());
    }
    if db_moved {
        println!("Renamed database: .daylog.db → .vitalog.db (sidecars handled).");
    }
}
```

No new method on `Config` is needed — `Config::load()` already does the right thing once the config dir has moved (Task 6's `resolve_config_path` picks the `vitalog/config.toml` path as soon as it exists).

- [ ] **Step 2: Add `pub mod migrate_cmd;` to `src/cli/mod.rs`**

Insert in alphabetical position among the existing `pub mod` lines.

- [ ] **Step 3: Add `Migrate` variant to `Commands` enum in `src/cli/mod.rs`**

```rust
    /// Migrate legacy daylog paths (config dir, database) to vitalog locations.
    /// Idempotent: safe to run multiple times.
    Migrate,
```

- [ ] **Step 4: Dispatch `Commands::Migrate` in `src/main.rs`**

In the main dispatch match, add:

```rust
        Some(Commands::Migrate) => vitalog::cli::migrate_cmd::execute(),
```

- [ ] **Step 5: Verify compile**

```bash
cargo check
```

Expected: clean.

- [ ] **Step 6: Write end-to-end integration test in `tests/migrate.rs`**

```rust
//! Integration test for `vitalog migrate`: simulate a daylog-era
//! install in a tempdir, run the migration, assert the new layout.

use std::process::Command;

#[test]
fn migrate_moves_config_dir_and_db() {
    let tmp = tempfile::TempDir::new().unwrap();

    // Lay out a daylog-era install rooted at $XDG_CONFIG_HOME = tmp.
    let xdg = tmp.path().join("xdg");
    let notes = tmp.path().join("notes");
    std::fs::create_dir_all(xdg.join("daylog")).unwrap();
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(
        xdg.join("daylog/config.toml"),
        format!("notes_dir = \"{}\"\n", notes.display()),
    )
    .unwrap();
    std::fs::write(notes.join(".daylog.db"), b"main").unwrap();
    std::fs::write(notes.join(".daylog.db-wal"), b"wal").unwrap();

    let bin = env!("CARGO_BIN_EXE_vitalog");
    let status = Command::new(bin)
        .arg("migrate")
        .env("XDG_CONFIG_HOME", &xdg)
        .env("HOME", tmp.path())  // belt + braces for macOS
        .status()
        .unwrap();
    assert!(status.success());

    assert!(!xdg.join("daylog").exists());
    assert!(xdg.join("vitalog/config.toml").is_file());
    assert!(!notes.join(".daylog.db").exists());
    assert!(!notes.join(".daylog.db-wal").exists());
    assert!(notes.join(".vitalog.db").is_file());
    assert!(notes.join(".vitalog.db-wal").is_file());
}

#[test]
fn migrate_is_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let xdg = tmp.path().join("xdg");
    std::fs::create_dir_all(&xdg).unwrap();

    let bin = env!("CARGO_BIN_EXE_vitalog");
    let out = Command::new(bin)
        .arg("migrate")
        .env("XDG_CONFIG_HOME", &xdg)
        .env("HOME", tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Nothing to do"),
        "expected 'Nothing to do' on a clean run; got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
```

Note: `XDG_CONFIG_HOME` is the cleanest override on Linux; on macOS, `dirs::config_dir()` reads `$HOME/Library/…` and ignores `XDG_CONFIG_HOME` by default. If running this test on macOS proves flaky, restrict it with `#[cfg(target_os = "linux")]` and document the constraint inline. Linux CI coverage is sufficient.

- [ ] **Step 7: Run the integration test**

```bash
cargo test --test migrate
```

Expected: PASS on Linux. If macOS flakes, gate the test with `#[cfg(target_os = "linux")]` and re-run.

- [ ] **Step 8: Commit**

```bash
git add src/cli/migrate_cmd.rs src/cli/mod.rs src/main.rs src/config.rs tests/migrate.rs
git commit -m "feat(cli): add vitalog migrate command"
```

---

## Task 8: README rename + attribution + drop demo line

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace every `daylog` with `vitalog` in `README.md`**

The substitution covers: title, badges (CI badge URL, crates.io badge URL — they'll be wrong until publication, but consistent), install commands, all CLI examples (`daylog init`, `daylog log …`, `daylog today`, etc.).

```bash
sed -i.bak 's/daylog/vitalog/g' README.md && rm README.md.bak
```

- [ ] **Step 2: Manually inspect the diff for over-substitution**

```bash
git diff README.md | head -100
```

Look for false positives (e.g., URLs to GitHub repo paths that shouldn't have changed if they reference daylog as a historical link). The fork attribution sentence — added next — *should* contain a literal `daylog`.

- [ ] **Step 3: Drop the demo GIF line near the top**

Remove the line:
```markdown
![vitalog demo](tapes/demo.gif)
```
(After Step 1 the path will say `vitalog demo` but the file is being deleted — drop the line entirely.)

- [ ] **Step 4: Add fork attribution near the top**

Add one line under the title or right after the badges, formatted to match the surrounding style:

```markdown
> Originally forked from [tfolkman/daylog](https://github.com/tfolkman/daylog).
```

- [ ] **Step 5: Verify the readme tests still pass**

```bash
cargo test readme
```

The test from Task 2 already asserts on `vitalog food`; with README updated, it passes.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs: rename README to vitalog and add fork attribution"
```

---

## Task 9: LICENSE — append vitalog modifications copyright

**Files:**
- Modify: `LICENSE`

- [ ] **Step 1: Edit `LICENSE`**

Replace lines 1–4 (the header):

```
MIT License

Copyright (c) 2026 Tyler
Copyright (c) 2026 Adrian Schmidt (vitalog modifications)
```

(Tyler's line stays untouched per MIT requirements; Adrian's line is appended below it.) The body (the "Permission is hereby granted…" paragraph through the WARRANTY section) stays exactly as-is.

- [ ] **Step 2: Verify the file**

```bash
head -6 LICENSE
```

Expected:
```
MIT License

Copyright (c) 2026 Tyler
Copyright (c) 2026 Adrian Schmidt (vitalog modifications)

Permission is hereby granted, free of charge, to any person obtaining a copy
```

- [ ] **Step 3: Commit**

```bash
git add LICENSE
git commit -m "docs: append vitalog modifications copyright to LICENSE"
```

---

## Task 10: Repo CLAUDE.md, AGENTS.md, justfile rename

**Files:**
- Modify: `CLAUDE.md`
- Modify: `AGENTS.md`
- Modify: `justfile`

- [ ] **Step 1: Replace `daylog` with `vitalog` in each file**

```bash
for f in CLAUDE.md AGENTS.md justfile; do
  sed -i.bak 's/daylog/vitalog/g' "$f" && rm "$f.bak"
done
```

- [ ] **Step 2: Inspect diffs**

```bash
git diff CLAUDE.md AGENTS.md justfile | head -200
```

Look for false positives. One known one: this repo's `CLAUDE.md` has `repository = "tfolkman/daylog"` mentions in the File Map context — Tyler's name in the path should remain `tfolkman/daylog` since that's a real upstream URL we still acknowledge. Verify those references are accurate after substitution; if any read incorrectly, fix manually.

- [ ] **Step 3: Run `just lint` to make sure justfile recipes still resolve**

```bash
just lint
```

Expected: rustfmt + clippy clean (this validates the justfile syntax, not just rust). If `just` complains about any recipe, fix.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md AGENTS.md justfile
git commit -m "docs: rename daylog references to vitalog in repo metadata"
```

---

## Task 11: CONTRIBUTING.md — rename + conventional-commits section

**Files:**
- Modify: `CONTRIBUTING.md`

- [ ] **Step 1: Substitute daylog → vitalog**

```bash
sed -i.bak 's/daylog/vitalog/g' CONTRIBUTING.md && rm CONTRIBUTING.md.bak
```

- [ ] **Step 2: Append a "Commit conventions" section**

Add at the end of `CONTRIBUTING.md`:

```markdown
## Commit conventions

vitalog uses [Conventional Commits](https://www.conventionalcommits.org/)
to drive automated releases. The accepted types are:

| Type       | Effect on next release |
|------------|------------------------|
| `feat`     | minor bump             |
| `fix`      | patch bump             |
| `perf`     | patch bump             |
| `BREAKING CHANGE:` footer (or `feat!:` / `fix!:`) | major bump |
| `docs`, `style`, `refactor`, `test`, `build`, `ci`, `chore` | no bump |

A commit subject line looks like:

```
feat(today): show streak counter on the dashboard

Some optional body explaining motivation and trade-offs.
```

Scope (the `(today)` part) is optional but encouraged. Use the module
or feature area, not a file path.
```

- [ ] **Step 3: Commit**

```bash
git add CONTRIBUTING.md
git commit -m "docs: rename CONTRIBUTING and document commit conventions"
```

---

## Task 12: Workspace CLAUDE.md (local-only, no commit)

**Files:**
- Modify: `/Users/bot/src/adrianschmidt/daylog-workspace/CLAUDE.md` (local untracked file outside any git repo)

- [ ] **Step 1: Update the PR-target instructions**

Replace `adrianschmidt/daylog` with `adrianschmidt/vitalog` everywhere in the workspace `CLAUDE.md`, including the prominent "🚨 STOP" header and the example `gh pr create` command.

- [ ] **Step 2: Update the smoke-testing section**

Change the binary name `daylog` to `vitalog` in command examples (e.g., `cargo run -- food …` stays the same, but `daylog food --date 2099-01-01` examples become `vitalog food --date 2099-01-01`). **Keep the example file paths** (`~/daylog-notes/2099-01-01.md`) as-is — Adrian's configured `notes_dir` is unchanged by this work.

- [ ] **Step 3: No git commit — the file is not tracked**

This task ends at "file saved." Note this manual edit in the PR description so the user knows it's been done.

---

## Task 13: Delete `tapes/` directory

**Files:**
- Delete: `tapes/` directory and contents
- Modify: `Cargo.toml` (drop `"tapes/"` from `exclude`)
- Modify: `README.md` (already handled in Task 8 if the line was dropped; double-check)

- [ ] **Step 1: Remove the directory**

```bash
git rm -r tapes/
```

- [ ] **Step 2: Update `Cargo.toml` `exclude`**

Change:
```toml
exclude = ["tapes/", ".claude/", ".github/"]
```
to:
```toml
exclude = [".claude/", ".github/"]
```

- [ ] **Step 3: Verify the README no longer references `tapes/`**

```bash
grep -n tapes README.md
```

Expected: no output (Task 8 dropped the demo line).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml tapes/
git commit -m "build: remove daylog demo gif (not being re-recorded for vitalog)"
```

---

## Task 14: `scripts/bump-cargo.sh`

**Files:**
- Create: `scripts/bump-cargo.sh`

- [ ] **Step 1: Create the script**

```bash
mkdir -p scripts
```

Write `scripts/bump-cargo.sh`:

```bash
#!/usr/bin/env bash
# Bump the [package] version in Cargo.toml to the value in $1, then
# regenerate Cargo.lock. Used by both the `prepare` workflow job and by
# semantic-release's @semantic-release/exec prepareCmd.
#
# Usage: scripts/bump-cargo.sh 1.2.3
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <new-version>" >&2
  exit 64
fi

new_version="$1"
sed -i.bak "s/^version = \".*\"/version = \"${new_version}\"/" Cargo.toml
rm Cargo.toml.bak

# Reconcile Cargo.lock with the bumped Cargo.toml. Use --offline first
# (fast, uses cached registry); fall back to a non-offline run if the
# cache is cold (e.g., on a fresh CI runner before the build job has
# warmed it).
cargo update --workspace --offline 2>/dev/null || cargo update --workspace

echo "bumped Cargo.toml + Cargo.lock to ${new_version}"
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x scripts/bump-cargo.sh
```

- [ ] **Step 3: Smoke-test it locally on a copy**

```bash
cp Cargo.toml /tmp/Cargo.toml.bak
scripts/bump-cargo.sh 9.9.9
grep '^version' Cargo.toml
# Expected: version = "9.9.9"
mv /tmp/Cargo.toml.bak Cargo.toml
cargo update --workspace --offline   # restore Cargo.lock
```

- [ ] **Step 4: Verify Cargo.toml is back to its previous version**

```bash
grep '^version' Cargo.toml
```

Expected: `version = "0.1.0"` (unchanged from before the smoke test).

- [ ] **Step 5: Commit**

```bash
git add scripts/bump-cargo.sh
git commit -m "ci: add scripts/bump-cargo.sh for semantic-release"
```

---

## Task 15: `release.config.js`

**Files:**
- Create: `release.config.js`

- [ ] **Step 1: Create the config**

```js
module.exports = {
  branches: ['main'],
  plugins: [
    '@semantic-release/commit-analyzer',
    '@semantic-release/release-notes-generator',
    '@semantic-release/changelog',
    ['@semantic-release/exec', {
      prepareCmd: 'scripts/bump-cargo.sh ${nextRelease.version}'
    }],
    ['@semantic-release/git', {
      assets: ['Cargo.toml', 'Cargo.lock', 'CHANGELOG.md'],
      message: 'chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}'
    }],
    ['@semantic-release/github', {
      assets: [
        { path: 'dist/vitalog-x86_64-unknown-linux-gnu/*', label: 'Linux x86_64' },
        { path: 'dist/vitalog-x86_64-apple-darwin/*',      label: 'macOS x86_64' },
        { path: 'dist/vitalog-aarch64-apple-darwin/*',     label: 'macOS ARM64' },
        { path: 'dist/vitalog-x86_64-pc-windows-msvc/*',   label: 'Windows x86_64' }
      ]
    }]
  ]
};
```

- [ ] **Step 2: Lint-parse it locally**

```bash
node --check release.config.js
```

Expected: no output (success).

- [ ] **Step 3: Commit**

```bash
git add release.config.js
git commit -m "ci: add semantic-release config"
```

---

## Task 16: Replace `.github/workflows/release.yml`

**Files:**
- Replace: `.github/workflows/release.yml`

The new workflow has four jobs (analyze, prepare, build × 4 matrix, publish), pins all external actions to commit SHAs with `# version` comments matching the existing `ci.yml` style, and uses the GitHub App for token minting.

- [ ] **Step 1: Resolve current commit SHAs for each external action**

The actions used by the new workflow:

- `actions/checkout@v4`
- `actions/setup-node@v4`
- `actions/upload-artifact@v4`
- `actions/download-artifact@v4`
- `actions/create-github-app-token@v1`
- `dtolnay/rust-toolchain@stable`
- `Swatinem/rust-cache@v2`

For consistency with `ci.yml`, look up each action's pinned SHA at implementation time. Quick lookup pattern:

```bash
gh api repos/actions/checkout/git/refs/tags/v4 --jq '.object.sha'
gh api repos/actions/setup-node/git/refs/tags/v4 --jq '.object.sha'
# ... etc.
```

Reuse SHAs already pinned in `ci.yml` where the action+version match.

- [ ] **Step 2: Write the new `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    branches: [main]
  workflow_dispatch:

concurrency:
  group: release
  cancel-in-progress: false

jobs:
  analyze:
    name: Analyze
    runs-on: ubuntu-latest
    permissions:
      contents: read
    outputs:
      version: ${{ steps.semrel.outputs.version }}
    steps:
      - uses: actions/create-github-app-token@<sha>  # v1
        id: app-token
        with:
          app-id: ${{ secrets.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}
      - uses: actions/checkout@<sha>  # v4
        with:
          fetch-depth: 0
          token: ${{ steps.app-token.outputs.token }}
      - uses: actions/setup-node@<sha>  # v4
        with:
          node-version: lts/*
      - name: Determine next version (dry run)
        id: semrel
        env:
          GITHUB_TOKEN: ${{ steps.app-token.outputs.token }}
        run: |
          set -euo pipefail
          # Capture the dry-run output and parse the next version, if any.
          out=$(npx --yes \
            -p semantic-release@latest \
            -p @semantic-release/changelog \
            -p @semantic-release/exec \
            -p @semantic-release/git \
            semantic-release --dry-run 2>&1) || true
          echo "$out"
          # The line we look for: "The next release version is X.Y.Z"
          version=$(printf '%s\n' "$out" | sed -nE 's/.*The next release version is ([0-9.]+).*/\1/p' | head -n1)
          if [ -n "$version" ]; then
            echo "version=$version" >> "$GITHUB_OUTPUT"
            echo "Next release: $version"
          else
            echo "No release needed."
          fi

  prepare:
    name: Prepare workspace
    needs: analyze
    if: needs.analyze.outputs.version != ''
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@<sha>  # v4
        with:
          fetch-depth: 1
      - uses: dtolnay/rust-toolchain@<sha>  # stable
      - uses: Swatinem/rust-cache@<sha>  # v2
      - name: Bump Cargo.toml + Cargo.lock
        run: scripts/bump-cargo.sh ${{ needs.analyze.outputs.version }}
      - name: Tar bumped manifest + lock
        run: tar czf bumped-workspace.tar.gz Cargo.toml Cargo.lock
      - uses: actions/upload-artifact@<sha>  # v4
        with:
          name: bumped-workspace
          path: bumped-workspace.tar.gz

  build:
    name: Build (${{ matrix.target }})
    needs: [analyze, prepare]
    if: needs.analyze.outputs.version != ''
    runs-on: ${{ matrix.os }}
    permissions:
      contents: read
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            binary: vitalog
            archive: tar
          - target: x86_64-apple-darwin
            os: macos-latest
            binary: vitalog
            archive: tar
          - target: aarch64-apple-darwin
            os: macos-latest
            binary: vitalog
            archive: tar
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            binary: vitalog.exe
            archive: zip
    steps:
      - uses: actions/checkout@<sha>  # v4
      - uses: actions/download-artifact@<sha>  # v4
        with:
          name: bumped-workspace
      - name: Apply bumped manifest + lock
        shell: bash
        run: tar xzf bumped-workspace.tar.gz
      - uses: dtolnay/rust-toolchain@<sha>  # stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@<sha>  # v2
      - name: Build
        run: cargo build --release --target ${{ matrix.target }}
      - name: Package (Unix)
        if: matrix.archive == 'tar'
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../vitalog-${{ matrix.target }}.tar.gz ${{ matrix.binary }}
      - name: Package (Windows)
        if: matrix.archive == 'zip'
        run: |
          cd target/${{ matrix.target }}/release
          7z a ../../../vitalog-${{ matrix.target }}.zip ${{ matrix.binary }}
      - uses: actions/upload-artifact@<sha>  # v4
        with:
          name: vitalog-${{ matrix.target }}
          path: vitalog-${{ matrix.target }}.*

  publish:
    name: Publish
    needs: [analyze, build]
    if: needs.analyze.outputs.version != ''
    runs-on: ubuntu-latest
    permissions:
      contents: write
      issues: write
      pull-requests: write
    steps:
      - uses: actions/create-github-app-token@<sha>  # v1
        id: app-token
        with:
          app-id: ${{ secrets.APP_ID }}
          private-key: ${{ secrets.APP_PRIVATE_KEY }}
      - uses: actions/checkout@<sha>  # v4
        with:
          fetch-depth: 0
          token: ${{ steps.app-token.outputs.token }}
      - uses: dtolnay/rust-toolchain@<sha>  # stable
      - uses: actions/setup-node@<sha>  # v4
        with:
          node-version: lts/*
      - uses: actions/download-artifact@<sha>  # v4
        with:
          path: dist
      - name: semantic-release
        env:
          GITHUB_TOKEN: ${{ steps.app-token.outputs.token }}
          GIT_AUTHOR_NAME: vitalog-release-bot
          GIT_AUTHOR_EMAIL: vitalog-release-bot@users.noreply.github.com
          GIT_COMMITTER_NAME: vitalog-release-bot
          GIT_COMMITTER_EMAIL: vitalog-release-bot@users.noreply.github.com
        run: |
          npx --yes \
            -p semantic-release@latest \
            -p @semantic-release/changelog \
            -p @semantic-release/exec \
            -p @semantic-release/git \
            semantic-release
```

Replace each `<sha>` with the resolved SHA from Step 1. Match the `# version` comment style used in `ci.yml` (e.g., `# v4`, `# stable`, `# v2`).

- [ ] **Step 3: Validate the workflow YAML locally**

```bash
# If `actionlint` is installed, use it; otherwise rely on GitHub's own
# parsing once pushed.
actionlint .github/workflows/release.yml || true
```

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: replace tag-triggered release.yml with semantic-release pipeline"
```

---

## Task 17: Pre-merge verification

Final pass before pushing the branch.

- [ ] **Step 1: Format, lint, and test**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all clean.

- [ ] **Step 2: Build the release binary and smoke-test the rename**

```bash
cargo build --release
./target/release/vitalog --version
./target/release/vitalog --help
./target/release/vitalog readme | head -20
```

Expected: version line says `vitalog x.y.z`; `--help` shows `vitalog` as the program name; `readme` prints the renamed README header.

- [ ] **Step 3: Smoke-test `vitalog migrate` against a temp install**

Following the workspace `CLAUDE.md` smoke-testing convention, do **not** point this at the real config. Use a tempdir-rooted fake on Linux:

```bash
TEMP=$(mktemp -d)
mkdir -p "$TEMP/xdg/daylog" "$TEMP/notes"
cat > "$TEMP/xdg/daylog/config.toml" <<EOF
notes_dir = "$TEMP/notes"
EOF
touch "$TEMP/notes/.daylog.db"
XDG_CONFIG_HOME="$TEMP/xdg" HOME="$TEMP" ./target/release/vitalog migrate
ls "$TEMP/xdg/" "$TEMP/notes/"
```

Expected: `xdg/vitalog/config.toml` exists; `notes/.vitalog.db` exists; the old paths are gone.

If this is run on macOS, set `HOME` and verify `dirs::config_dir()` resolves under it (or skip and rely on Linux CI).

- [ ] **Step 4: Verify the new release.yml has no `<sha>` placeholders left**

```bash
grep -n '<sha>' .github/workflows/release.yml
```

Expected: no output.

- [ ] **Step 5: If issues found, fix and commit; otherwise proceed**

---

## Task 18: Tag cleanup + push branch + open PR

- [ ] **Step 1: Delete the local `v0.1.0` tag**

```bash
git tag -d v0.1.0
```

(Origin doesn't have this tag, so no force-push concerns.)

- [ ] **Step 2: Push the `vitalog-rename` branch to origin**

```bash
git push -u origin vitalog-rename
```

(Note: `origin` still resolves to `adrianschmidt/daylog` at this stage. The repo rename happens in Task 19.)

- [ ] **Step 3: Open a PR targeting the fork**

Per the workspace `CLAUDE.md` guardrail, this MUST target `adrianschmidt/<repo>`, not upstream:

```bash
gh pr create -R adrianschmidt/daylog \
  --base main \
  --head adrianschmidt:vitalog-rename \
  --title "Rename to vitalog + automated release pipeline" \
  --body "$(cat <<'EOF'
Renames the fork to **vitalog** and stands up an automated release
pipeline using semantic-release + a dedicated GitHub App
(`vitalog-release-bot`) that bypasses the `main` branch ruleset.

See `docs/superpowers/specs/2026-05-05-vitalog-rename-and-release-pipeline-design.md`
for the full spec and `docs/superpowers/plans/2026-05-05-vitalog-rename-and-release-pipeline.md`
for the task-by-task plan.

Manual step performed outside the diff:

- Workspace `CLAUDE.md` (one directory up, untracked) updated to point
  PRs at `adrianschmidt/vitalog`.

To complete the rollout after merging:

1. Rename the GitHub repo `adrianschmidt/daylog` → `adrianschmidt/vitalog`
   (App installation + ruleset bypass follow automatically).
2. `git remote set-url origin git@github.com:adrianschmidt/vitalog.git` locally.
3. The merge will trigger `release.yml` and produce `vitalog v1.0.0`.
EOF
)"
```

- [ ] **Step 4: Wait for CI to pass on the PR**

Watch `gh pr checks` or the PR page. `release.yml`'s `analyze` step runs but should output an empty version (no commits since last tag — except actually, with `v0.1.0` deleted locally and absent on origin, the analyze step *will* find a release-worthy diff and try to bump). This is fine on the PR (the PR doesn't trigger `release.yml`'s push-to-main path; only merge does).

Confirm `ci.yml` passes (fmt, clippy, test, audit, deny on three OS).

---

## Task 19: GitHub-side repo rename (manual, performed outside Claude)

- [ ] **Step 1: User renames the repo**

In the GitHub UI: `adrianschmidt/daylog` → Settings → "Rename" → `vitalog`.

GitHub auto-redirects the old URL. The App installation + Ruleset bypass follow automatically (keyed on stable internal repo IDs).

- [ ] **Step 2: User updates the local remote**

```bash
git remote set-url origin git@github.com:adrianschmidt/vitalog.git
git remote -v   # verify
```

- [ ] **Step 3: User confirms the PR is now on `adrianschmidt/vitalog`**

```bash
gh pr view --web
```

The PR (still open, not yet merged) follows the rename automatically.

---

## Task 20: Merge + verify first release

- [ ] **Step 1: Merge the PR**

Via the GitHub UI, or:

```bash
gh pr merge --squash   # or --merge, per Adrian's preference
```

(Actually use `--merge` to preserve the per-task commits — semantic-release reads them to build the changelog. Squash would collapse all 80+ feature commits + the rename commits into one, breaking changelog generation.)

```bash
gh pr merge --merge
```

- [ ] **Step 2: Watch `release.yml` run on `main`**

```bash
gh run watch
```

Expected: `analyze` outputs `version=1.0.0`; `prepare` produces the artifact; four matrix builds succeed; `publish` creates tag `v1.0.0`, commits CHANGELOG + bumped Cargo.toml/Cargo.lock with `[skip ci]`, creates the GitHub Release with four labeled binary assets.

- [ ] **Step 3: Verify the first release**

```bash
gh release view v1.0.0
gh release download v1.0.0 -p '*linux*' -D /tmp/v1-test
tar xzf /tmp/v1-test/vitalog-x86_64-unknown-linux-gnu.tar.gz -C /tmp/v1-test
/tmp/v1-test/vitalog --version
```

Expected: `vitalog 1.0.0`.

- [ ] **Step 4: Verify the bypass + loop prevention**

```bash
gh api repos/adrianschmidt/vitalog/commits/main --jq '.commit.author.name'
```

Expected: `vitalog-release-bot` (the App identity), confirming the ruleset bypass worked.

```bash
gh run list --workflow=release.yml --limit 5
```

Expected: exactly one run for the originating push (the `[skip ci]` on the bot commit prevented re-trigger).

- [ ] **Step 5: Verify CHANGELOG content**

```bash
gh api repos/adrianschmidt/vitalog/contents/CHANGELOG.md --jq '.content' | base64 -d | head -50
```

Expected: a generated CHANGELOG with sections for Features, Bug Fixes, Performance Improvements, etc., listing the conventional commits since the fork diverged.

- [ ] **Step 6: If anything fails**

- A specific job failed: re-run that job from the Actions UI; the pipeline is idempotent because the bot's push (if any) has `[skip ci]` and the gating logic respects existing tags. If `publish` already pushed tag `v1.0.0` but failed to upload assets, manually upload via `gh release upload v1.0.0 dist/*`.
- Workflow didn't trigger: confirm the merge actually pushed to `main` (not a different branch) and the workflow file is on the merged commit.
- semantic-release picked the wrong version: rare, but check the commit message log for missing or malformed conventional types; fix forward with a follow-up commit and `chore(release):` will re-run.

---

## Done criteria

The work is complete when:

- `vitalog v1.0.0` exists as a GitHub Release on `adrianschmidt/vitalog` with four labeled binary assets attached.
- `CHANGELOG.md` exists on `main` with the 80+ conventional commits documented.
- `Cargo.toml` on `main` says `name = "vitalog"`, `version = "1.0.0"`, `repository = "https://github.com/adrianschmidt/vitalog"`.
- A fresh clone + `cargo install --path .` produces a `vitalog` binary that runs `vitalog --version` reporting `1.0.0`.
- `vitalog migrate` in a tempdir-rooted fake `daylog` install moves the config and DB correctly.
- The first release commit on `main` was authored by `vitalog-release-bot` (proving the ruleset bypass).
- Exactly one `release.yml` run is visible for the originating merge (proving `[skip ci]`).
