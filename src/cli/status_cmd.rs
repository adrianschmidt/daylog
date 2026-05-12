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

    let output = assemble_status(&conn, config, &registry)?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub fn assemble_status(
    conn: &rusqlite::Connection,
    config: &Config,
    registry: &[Box<dyn modules::Module>],
) -> Result<serde_json::Value> {
    let today = config.effective_today();

    let mut output = serde_json::json!({
        "effective_date": &today,
        "day_start_hour": config.day_start_hour,
        "weight_unit": config.weight_unit.to_string(),
    });
    if let Some(day_data) = db::load_today(conn, &today)? {
        output["today"] = day_data;
    }

    for module in registry {
        if let Some(status) = module.status_json(conn, config) {
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

    let nutrition = db::nutrition_status(conn)?;
    output["nutrition_db"] = serde_json::json!({
        "foods_count": nutrition.foods_count,
        "last_synced": nutrition.last_synced,
    });

    // Reminders.
    let reminders_defs = crate::reminders::load_reminders(config)?;
    let eval = if reminders_defs.is_empty() {
        crate::reminders::EvaluationResult::default()
    } else {
        crate::reminders::evaluate(
            conn,
            config.effective_today_date(),
            chrono::Local::now().time(),
            &reminders_defs,
            config,
        )?
    };
    let (rs, warns) = crate::reminders::to_json(&eval.reminders, &eval.warnings);
    output["reminders"] = rs;
    output["reminder_warnings"] = warns;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn config_in(notes_dir: &std::path::Path, reminders_toml: &str) -> Config {
        let toml_str = format!(
            r#"
notes_dir = "{}"
time_format = "24h"
weight_unit = "kg"

[metrics]
la_min = {{ display = "Lactic acid (min)", color = "red" }}

{reminders_toml}
"#,
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    /// Run the body of execute() but return the JSON value instead of
    /// printing it. Used by tests to assert on the shape without
    /// scraping stdout.
    fn build_status_json(config: &Config) -> Result<serde_json::Value> {
        let registry = crate::modules::build_registry(config);
        let db_path = config.db_path();
        let conn = db::open_rw(&db_path)?;
        db::init_db(&conn, &registry)?;
        crate::modules::validate_module_tables(&registry)?;
        let _ = crate::materializer::sync_all(&conn, &config.notes_dir_path(), config, &registry);
        super::assemble_status(&conn, config, &registry)
    }

    #[test]
    fn status_json_contains_empty_reminders_when_none_configured() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "");
        let v = build_status_json(&config).unwrap();
        assert!(v["reminders"].is_array(), "got:\n{v}");
        assert_eq!(v["reminders"].as_array().unwrap().len(), 0);
        assert!(v["reminder_warnings"].is_array(), "got:\n{v}");
    }

    #[test]
    fn status_json_includes_due_reminder() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(
            dir.path(),
            r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
        );
        let v = build_status_json(&config).unwrap();
        let arr = v["reminders"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "lactic_acid");
        assert_eq!(arr[0]["due"], true);
        assert!(arr[0]["last_done"].is_null());
    }

    #[test]
    fn status_json_unknown_metric_target_warns() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(
            dir.path(),
            r#"
[reminders.typo]
display = "Typo"
interval_days = 1
watch = "metric"
target = "nonexistent"
"#,
        );
        let v = build_status_json(&config).unwrap();
        let warns = v["reminder_warnings"].as_array().unwrap();
        assert_eq!(warns.len(), 1);
        assert!(warns[0].as_str().unwrap().contains("nonexistent"));
    }

    #[test]
    fn status_json_includes_time_gates_for_each_reminder() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(
            dir.path(),
            r#"
[reminders.evening]
display = "Evening"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "18:00"
not_after = "23:00"
"#,
        );
        let v = build_status_json(&config).unwrap();
        let r = &v["reminders"][0];
        assert_eq!(r["not_before"], "18:00");
        assert_eq!(r["not_after"], "23:00");
    }
}
