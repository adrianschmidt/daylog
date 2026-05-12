//! `vitalog status` — print today's data as JSON.
//!
//! Aggregates day-level fields, module status, pending sleep, and
//! nutrition-DB status into a single JSON object. Mirrors `today_cmd`
//! in that it syncs notes to the DB before reading so just-logged data
//! is visible (see issue #27).

use color_eyre::eyre::Result;

use crate::config::Config;
use crate::{db, materializer, modules, state, time};

pub fn execute(config: &Config) -> Result<()> {
    let registry = modules::build_registry(config);
    let db_path = config.db_path();

    if !db_path.exists() {
        color_eyre::eyre::bail!(
            "Database not found at {}. Run `vitalog init` or `vitalog sync` first.",
            db_path.display()
        );
    }

    let conn = db::open_rw(&db_path)?;
    db::init_db(&conn, &registry)?;
    modules::validate_module_tables(&registry)?;
    let _ = materializer::sync_all(&conn, &config.notes_dir_path(), config, &registry);

    let today = config.effective_today();

    let mut output = serde_json::json!({
        "effective_date": &today,
        "day_start_hour": config.day_start_hour,
        "weight_unit": config.weight_unit.to_string(),
    });
    if let Some(day_data) = db::load_today(&conn, &today)? {
        output["today"] = day_data;
    }

    for module in &registry {
        if let Some(status) = module.status_json(&conn, config) {
            output[module.id()] = status;
        }
    }

    let pending = state::load(&config.notes_dir_path());
    if let Some(p) = pending.sleep_start {
        output["pending"] = serde_json::json!({
            "sleep_start": {
                "bedtime": time::format_time(p.bedtime, config.time_format),
                "recorded_at": p.recorded_at.to_rfc3339(),
            }
        });
    }

    let nutrition = db::nutrition_status(&conn)?;
    output["nutrition_db"] = serde_json::json!({
        "foods_count": nutrition.foods_count,
        "last_synced": nutrition.last_synced,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
