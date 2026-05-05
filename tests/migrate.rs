//! Integration test for `vitalog migrate`: simulate a daylog-era
//! install in a tempdir, run the migration, assert the new layout.

#[cfg(target_os = "linux")]
use std::process::Command;

#[test]
#[cfg(target_os = "linux")]
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
        .env("HOME", tmp.path())
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
#[cfg(target_os = "linux")]
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
