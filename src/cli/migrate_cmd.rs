//! `vitalog migrate` — move legacy daylog config dir + database file
//! to the vitalog locations. Idempotent; refuses to overwrite when
//! both old and new locations contain data.

use color_eyre::Result;
use std::path::Path;

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
        Err(_) if !cfg_moved => false, // no config anywhere → nothing to migrate
        Err(e) => return Err(e),       // config existed but failed to load
    };

    print_summary(cfg_moved, &legacy_cfg, &current_cfg, db_moved);
    Ok(())
}

fn print_summary(cfg_moved: bool, from: &Path, to: &Path, db_moved: bool) {
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
