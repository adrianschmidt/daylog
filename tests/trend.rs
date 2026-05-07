//! End-to-end tests for `vitalog trend`.

use chrono::NaiveDate;
use vitalog::cli::trend_cmd::{assemble, render_chart, render_compact, render_json};
use vitalog::config::Config;
use vitalog::db;
use vitalog::modules;

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
training = false
trends = true
climbing = false

[metrics]
resting_hr = {{ display = "Resting HR", color = "red", unit = "bpm" }}

[exercises]
"#
    );
    let config: Config = toml::from_str(&toml_str).unwrap();
    (dir, config)
}

fn write_note(notes_dir: &std::path::Path, date: &str, body: &str) {
    std::fs::write(notes_dir.join(format!("{date}.md")), body).unwrap();
}

fn sync(config: &Config) {
    let registry = modules::build_registry(config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), config, &registry).unwrap();
}

#[test]
fn trend_weight_chart_includes_axis_and_stats() {
    let (dir, config) = setup();
    write_note(
        dir.path(),
        "2026-04-25",
        "---\ndate: 2026-04-25\nweight: 120.0\n---\n",
    );
    write_note(
        dir.path(),
        "2026-04-26",
        "---\ndate: 2026-04-26\nweight: 120.5\n---\n",
    );
    write_note(
        dir.path(),
        "2026-04-28",
        "---\ndate: 2026-04-28\nweight: 121.0\n---\n",
    );
    sync(&config);

    let conn = db::open_ro(&config.db_path()).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
    let data = assemble("weight", 4, &config, &conn, today).unwrap();

    assert_eq!(data.points.len(), 4);
    assert_eq!(
        data.points[0].0,
        NaiveDate::from_ymd_opt(2026, 4, 25).unwrap()
    );
    assert_eq!(data.points[2].1, None); // 04-27 is a gap
    assert_eq!(data.stats.count, 3);

    let chart = render_chart(&data);
    assert!(chart.contains("weight (last 4 days, kg)"), "got:\n{chart}");
    assert!(chart.contains("┤"));
    assert!(chart.contains("04-25"));
    assert!(chart.contains("04-28"));
    assert!(chart.contains("mean:"));
    assert!(chart.contains("linear trend:"));
}

#[test]
fn trend_metric_json_round_trip() {
    let (dir, config) = setup();
    write_note(
        dir.path(),
        "2026-04-27",
        "---\ndate: 2026-04-27\nresting_hr: 58\n---\n",
    );
    write_note(
        dir.path(),
        "2026-04-28",
        "---\ndate: 2026-04-28\nresting_hr: 60\n---\n",
    );
    sync(&config);

    let conn = db::open_ro(&config.db_path()).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
    let data = assemble("resting_hr", 2, &config, &conn, today).unwrap();
    let v = render_json(&data);

    assert_eq!(v["field"], "resting_hr");
    assert_eq!(v["display"], "Resting HR");
    assert_eq!(v["unit"], "bpm");
    let pts = v["points"].as_array().unwrap();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0]["value"], 58.0);
    assert_eq!(pts[1]["value"], 60.0);
    assert_eq!(v["stats"]["count"], 2);
}

#[test]
fn trend_compact_renders_one_line_plus_stats() {
    let (dir, config) = setup();
    for (date, w) in &[
        ("2026-04-25", 120.0),
        ("2026-04-26", 120.5),
        ("2026-04-27", 121.0),
        ("2026-04-28", 121.5),
    ] {
        write_note(
            dir.path(),
            date,
            &format!("---\ndate: {date}\nweight: {w}\n---\n"),
        );
    }
    sync(&config);

    let conn = db::open_ro(&config.db_path()).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
    let data = assemble("weight", 4, &config, &conn, today).unwrap();
    let s = render_compact(&data);

    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 3, "compact output should be 3 lines: {s}");
    assert!(lines[0].starts_with("weight (4d, kg): "), "got: {s}");
    assert!(lines[1].starts_with("mean "), "got: {s}");
    assert!(lines[2].starts_with("slope "), "got: {s}");
}

#[test]
fn trend_unknown_field_errors() {
    let (_dir, config) = setup();
    sync(&config);
    let conn = db::open_ro(&config.db_path()).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
    let err = assemble("not_a_field", 7, &config, &conn, today).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not_a_field"), "got: {msg}");
    assert!(msg.contains("weight"), "got: {msg}");
    assert!(msg.contains("resting_hr"), "got: {msg}");
}
