//! End-to-end test for `daylog today`.

use chrono::NaiveDate;
use daylog::cli::today_cmd::{assemble, render_json, render_text};
use daylog::config::Config;
use daylog::db;
use daylog::goals::load_goals;
use daylog::modules;

fn setup() -> (tempfile::TempDir, Config) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().display().to_string().replace('\\', "/");
    let toml_str = format!(
        r#"
notes_dir = "{path}"
time_format = "24h"
weight_unit = "kg"

[modules]
dashboard = true
training = true
trends = true
climbing = false

[metrics]
resting_hr = {{ display = "Resting HR", color = "red", unit = "bpm" }}

[exercises]
squat = {{ display = "Squat", color = "cyan" }}
"#
    );
    let config: Config = toml::from_str(&toml_str).unwrap();
    (dir, config)
}

fn write_note(notes_dir: &std::path::Path, date: &str, body: &str) {
    std::fs::write(notes_dir.join(format!("{date}.md")), body).unwrap();
}

fn write_goals(notes_dir: &std::path::Path, body: &str) {
    std::fs::write(notes_dir.join("goals.md"), body).unwrap();
}

#[test]
fn end_to_end_today_text_and_json() {
    let (dir, config) = setup();

    write_note(
        dir.path(),
        "2026-04-29",
        "---\ndate: 2026-04-29\nweight: 120.2\n---\n\n## Food\n",
    );
    write_note(
        dir.path(),
        "2026-04-30",
        "---\n\
         date: 2026-04-30\n\
         weight: 121.5\n\
         sleep: \"23:00-05:24\"\n\
         bp_morning_sys: 138\n\
         bp_morning_dia: 88\n\
         bp_morning_pulse: 70\n\
         resting_hr: 58\n\
         ---\n\n\
         ## Food\n\
         - **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n\
         - **12:00** Pasta (500 kcal, 18.0g protein, 80.0g carbs, 10.0g fat)\n\
         - **18:00** Soup (813 kcal, 117.0g protein, 5.0g carbs, 34.0g fat)\n",
    );
    write_goals(
        dir.path(),
        "---\nkcal_min: 1900\nkcal_max: 2200\nprotein_min: 140\nweight_target: 110\n---\n\n# notes\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    daylog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();

    let text = render_text(&summary, &goals, false);
    assert!(text.contains("2026-04-30 — Daily summary"), "got:\n{text}");
    assert!(text.contains("Calories:"));
    assert!(text.contains("1513"));
    assert!(text.contains("1900–2200 kcal"));
    assert!(text.contains("Protein:"));
    assert!(text.contains("147"));
    assert!(text.contains("≥140 g"));
    assert!(text.contains("Carbs:"));
    assert!(text.contains("Weight:    121.5 kg"));
    assert!(text.contains("Δ +1.3 vs yesterday"));
    assert!(text.contains("Sleep:     6h 24min"));
    assert!(text.contains("BP morning:   138/88 (pulse 70)"));
    assert!(text.contains("Resting HR: 58"));

    let v = render_json(&summary, &goals);
    assert_eq!(v["date"], "2026-04-30");
    assert_eq!(v["metrics"]["kcal"]["min"], 1900.0);
    assert_eq!(v["metrics"]["kcal"]["max"], 2200.0);
    assert_eq!(v["metrics"]["protein"]["min"], 140.0);
    assert_eq!(v["metrics"]["weight"]["delta_vs_date"], "2026-04-29");
    assert_eq!(v["bp_morning"]["sys"], 138);
    assert_eq!(v["sleep"]["hours"], 6.4);
    assert_eq!(v["goals_present"], true);
}

#[test]
fn end_to_end_today_no_goals_emits_hint() {
    let (dir, config) = setup();

    write_note(
        dir.path(),
        "2026-04-30",
        "---\ndate: 2026-04-30\n---\n\n## Food\n- **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    daylog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();
    assert!(!goals.present);

    let text = render_text(&summary, &goals, false);
    assert!(text.contains("No goals defined"), "got:\n{text}");
}
