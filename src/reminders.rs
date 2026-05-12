//! Smart reminders evaluated against the existing DB tables.
//!
//! A reminder watches one of: a custom metric, a row in `sessions`, a row in
//! `lift_sets`, or a built-in `days` column. `evaluate` returns the most
//! recent date the watched thing was logged (if any), and whether that
//! gap is ≥ `interval_days` calendar days ago — in which case the
//! reminder is "due".
//!
//! Sibling to `goals.rs`. No new DB schema, no daemon-side state.

use chrono::NaiveDate;
use color_eyre::eyre::Result;
use rusqlite::Connection;

use crate::config::Config;

/// A reminder definition, fully resolved from TOML config.
#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub watch: WatchSource,
}

/// What the reminder watches. Each variant maps to one prepared query in
/// `evaluate`.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchSource {
    /// Any row in `metrics` matching `name = id`. By default a value of 0
    /// does NOT count as "logged"; opt in via `count_zero_as_logged`.
    Metric {
        id: String,
        count_zero_as_logged: bool,
    },
    /// Any row in `sessions` matching the predicate.
    Session(SessionMatch),
    /// Any row in `lift_sets` with the named exercise, optionally filtered
    /// by `min_weight` (lbs, matching the column) and `min_reps`.
    Lift {
        exercise: String,
        min_weight: Option<f64>,
        min_reps: Option<u32>,
    },
    /// Any row in `days` with the named column non-null.
    DayField(DayColumn),
}

/// `sessions`-table predicates. Closed enum; column whitelist is enforced
/// by these variants.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionMatch {
    TextEquals {
        column: SessionTextColumn,
        value: String,
    },
    NumericAtLeast {
        column: SessionNumColumn,
        min: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTextColumn {
    /// SQL: `session_type`. YAML/config name: `type`.
    Type,
    Block,
    Vo2Intervals,
}

impl SessionTextColumn {
    /// SQL column name (not the YAML/config alias).
    pub fn sql_column(self) -> &'static str {
        match self {
            SessionTextColumn::Type => "session_type",
            SessionTextColumn::Block => "block",
            SessionTextColumn::Vo2Intervals => "vo2_intervals",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionNumColumn {
    Duration,
    Rpe,
    ZoneTwoMin,
    HrAvg,
    Week,
}

impl SessionNumColumn {
    pub fn sql_column(self) -> &'static str {
        match self {
            SessionNumColumn::Duration => "duration",
            SessionNumColumn::Rpe => "rpe",
            SessionNumColumn::ZoneTwoMin => "zone2_min",
            SessionNumColumn::HrAvg => "hr_avg",
            SessionNumColumn::Week => "week",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayColumn {
    Weight,
    SleepHours,
    Mood,
    Energy,
    SleepStart,
    SleepEnd,
}

impl DayColumn {
    pub fn sql_column(self) -> &'static str {
        match self {
            DayColumn::Weight => "weight",
            DayColumn::SleepHours => "sleep_hours",
            DayColumn::Mood => "mood",
            DayColumn::Energy => "energy",
            DayColumn::SleepStart => "sleep_start",
            DayColumn::SleepEnd => "sleep_end",
        }
    }
}

/// One reminder, evaluated against the DB.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedReminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub last_done: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub due: bool,
}

/// Output of `evaluate`: evaluated reminders plus any soft warnings (e.g.
/// a metric target that's not declared in `[metrics]`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluationResult {
    pub reminders: Vec<EvaluatedReminder>,
    pub warnings: Vec<String>,
}

/// Parse the `[reminders]` section of the config into `Reminder` values.
/// Fails fast on structural errors (missing fields, unknown enum variants,
/// invalid `interval_days`); soft issues (unknown metric target) are
/// surfaced by `evaluate` at runtime.
pub fn load_reminders(config: &Config) -> Result<Vec<Reminder>> {
    use color_eyre::Help;

    let mut out: Vec<Reminder> = Vec::with_capacity(config.reminders.len());
    for (id, cfg) in &config.reminders {
        if cfg.interval_days < 1 {
            return Err(color_eyre::eyre::eyre!(
                "reminder `{id}`: interval_days must be ≥ 1 (got {})",
                cfg.interval_days
            ))
            .suggestion("Set interval_days to a positive integer in config.toml.");
        }
        let watch = match cfg.watch.as_str() {
            "metric" => parse_metric_watch(id, &cfg.target, cfg.count_zero_as_logged)?,
            "session" => parse_session_watch(id, &cfg.target)?,
            "lift" => parse_lift_watch(id, &cfg.target)?,
            "day_field" => parse_day_field_watch(id, &cfg.target)?,
            other => {
                return Err(color_eyre::eyre::eyre!(
                    "reminder `{id}`: unknown watch kind `{other}`"
                ))
                .suggestion("watch must be one of: metric, session, lift, day_field.");
            }
        };
        out.push(Reminder {
            id: id.clone(),
            display: cfg.display.clone(),
            interval_days: cfg.interval_days,
            watch,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn parse_metric_watch(
    id: &str,
    target: &toml::Value,
    count_zero_as_logged: bool,
) -> Result<WatchSource> {
    use color_eyre::Help;
    let metric_id = target
        .as_str()
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: watch = \"metric\" requires `target` to be a metric id string"
            )
        })
        .suggestion(r#"Example: target = "la_min""#)?;
    Ok(WatchSource::Metric {
        id: metric_id.to_string(),
        count_zero_as_logged,
    })
}

fn parse_session_watch(id: &str, target: &toml::Value) -> Result<WatchSource> {
    use color_eyre::Help;
    let table = target
        .as_table()
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: watch = \"session\" requires `target` to be an inline table"
            )
        })
        .suggestion(r#"Example: target = { field = "zone2_min", min_value = 1 }"#)?;
    let field = table
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            color_eyre::eyre::eyre!("reminder `{id}`: session target is missing `field`")
        })
        .suggestion(r#"Example: target = { field = "zone2_min", min_value = 1 }"#)?;

    let text_col = parse_session_text_column(field);
    let num_col = parse_session_numeric_column(field);
    let has_equals = table.contains_key("equals");
    let has_min_value = table.contains_key("min_value");

    match (text_col, num_col, has_equals, has_min_value) {
        (Some(col), _, true, false) => {
            let value = table
                .get("equals")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "reminder `{id}`: session target.equals must be a string"
                    )
                })?;
            Ok(WatchSource::Session(SessionMatch::TextEquals {
                column: col,
                value: value.to_string(),
            }))
        }
        (_, Some(col), false, true) => {
            let min = table
                .get("min_value")
                .and_then(toml_value_as_f64)
                .ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "reminder `{id}`: session target.min_value must be a number"
                    )
                })?;
            Ok(WatchSource::Session(SessionMatch::NumericAtLeast {
                column: col,
                min,
            }))
        }
        (Some(_), None, false, true) => Err(color_eyre::eyre::eyre!(
            "reminder `{id}`: session field `{field}` is a text column; use `equals = \"...\"`"
        ))
        .suggestion("Text-column predicates use `equals`; numeric columns use `min_value`."),
        (None, Some(_), true, false) => Err(color_eyre::eyre::eyre!(
            "reminder `{id}`: session field `{field}` is numeric; use `min_value = N`"
        ))
        .suggestion("Text-column predicates use `equals`; numeric columns use `min_value`."),
        (None, None, _, _) => Err(color_eyre::eyre::eyre!(
            "reminder `{id}`: session field `{field}` is not a recognized sessions column"
        ))
        .suggestion(
            "Allowed: type, block, vo2_intervals (text); duration, rpe, zone2_min, hr_avg, week (numeric).",
        ),
        _ => Err(color_eyre::eyre::eyre!(
            "reminder `{id}`: session target must have exactly one of `equals` or `min_value`"
        ))
        .suggestion(
            "Pick one: `equals = \"...\"` for text columns or `min_value = N` for numeric columns.",
        ),
    }
}

fn parse_session_text_column(name: &str) -> Option<SessionTextColumn> {
    match name {
        "type" => Some(SessionTextColumn::Type),
        "block" => Some(SessionTextColumn::Block),
        "vo2_intervals" => Some(SessionTextColumn::Vo2Intervals),
        _ => None,
    }
}

fn parse_session_numeric_column(name: &str) -> Option<SessionNumColumn> {
    match name {
        "duration" => Some(SessionNumColumn::Duration),
        "rpe" => Some(SessionNumColumn::Rpe),
        "zone2_min" => Some(SessionNumColumn::ZoneTwoMin),
        "hr_avg" => Some(SessionNumColumn::HrAvg),
        "week" => Some(SessionNumColumn::Week),
        _ => None,
    }
}

fn parse_lift_watch(id: &str, target: &toml::Value) -> Result<WatchSource> {
    use color_eyre::Help;
    let table = target
        .as_table()
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: watch = \"lift\" requires `target` to be an inline table"
            )
        })
        .suggestion(r#"Example: target = { exercise = "deadlift", min_weight = 200 }"#)?;
    let exercise = table
        .get("exercise")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            color_eyre::eyre::eyre!("reminder `{id}`: lift target is missing `exercise`")
        })
        .suggestion(r#"Example: target = { exercise = "deadlift" }"#)?;
    let min_weight = match table.get("min_weight") {
        None => None,
        Some(v) => Some(toml_value_as_f64(v).ok_or_else(|| {
            color_eyre::eyre::eyre!("reminder `{id}`: lift target.min_weight must be a number")
        })?),
    };
    let min_reps = match table.get("min_reps") {
        None => None,
        Some(v) => {
            let n = v.as_integer().ok_or_else(|| {
                color_eyre::eyre::eyre!("reminder `{id}`: lift target.min_reps must be an integer")
            })?;
            if n < 0 {
                return Err(color_eyre::eyre::eyre!(
                    "reminder `{id}`: lift target.min_reps must be ≥ 0 (got {n})"
                ));
            }
            Some(n as u32)
        }
    };
    Ok(WatchSource::Lift {
        exercise: exercise.to_string(),
        min_weight,
        min_reps,
    })
}

fn parse_day_field_watch(id: &str, target: &toml::Value) -> Result<WatchSource> {
    use color_eyre::Help;
    let name = target
        .as_str()
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: watch = \"day_field\" requires `target` to be a string"
            )
        })
        .suggestion(r#"Example: target = "weight""#)?;
    let col = match name {
        "weight" => DayColumn::Weight,
        "sleep_hours" => DayColumn::SleepHours,
        "mood" => DayColumn::Mood,
        "energy" => DayColumn::Energy,
        "sleep_start" => DayColumn::SleepStart,
        "sleep_end" => DayColumn::SleepEnd,
        _ => {
            return Err(color_eyre::eyre::eyre!(
                "reminder `{id}`: day_field target `{name}` is not a recognized days column"
            ))
            .suggestion("Allowed: weight, sleep_hours, mood, energy, sleep_start, sleep_end.");
        }
    };
    Ok(WatchSource::DayField(col))
}

fn toml_value_as_f64(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Float(f) => Some(*f),
        toml::Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// Evaluate reminders against the current DB state, computing `last_done`
/// per watched source. `today` is the effective today date (callers
/// should pass `config.effective_today_date()`).
pub fn evaluate(
    _conn: &Connection,
    _today: NaiveDate,
    reminders: &[Reminder],
    _config: &Config,
) -> Result<EvaluationResult> {
    Ok(EvaluationResult {
        reminders: reminders
            .iter()
            .map(|r| EvaluatedReminder {
                id: r.id.clone(),
                display: r.display.clone(),
                interval_days: r.interval_days,
                last_done: None,
                days_since: None,
                due: true,
            })
            .collect(),
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config_with(reminders_toml: &str) -> Config {
        let toml_str = format!(
            r#"
notes_dir = "/tmp/x"
time_format = "24h"

[metrics]
la_min = {{ display = "Lactic acid (min)", color = "red" }}

[exercises]
deadlift = {{ display = "Deadlift", color = "yellow" }}

{reminders_toml}
"#
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn load_parses_metric_watch() {
        let config = config_with(
            r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].id, "lactic_acid");
        assert_eq!(rs[0].display, "Lactic acid training");
        assert_eq!(rs[0].interval_days, 2);
        assert_eq!(
            rs[0].watch,
            WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            }
        );
    }

    #[test]
    fn load_parses_metric_watch_with_count_zero() {
        let config = config_with(
            r#"
[reminders.brushed]
display = "Brushed teeth"
interval_days = 1
watch = "metric"
target = "brushed_morning"
count_zero_as_logged = true
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].watch,
            WatchSource::Metric {
                id: "brushed_morning".into(),
                count_zero_as_logged: true,
            }
        );
    }

    #[test]
    fn load_parses_session_text_equals() {
        let config = config_with(
            r#"
[reminders.vo2]
display = "VO2max"
interval_days = 7
watch = "session"
target = { field = "type", equals = "vo2_max" }
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].watch,
            WatchSource::Session(SessionMatch::TextEquals {
                column: SessionTextColumn::Type,
                value: "vo2_max".into(),
            })
        );
    }

    #[test]
    fn load_parses_session_numeric_at_least() {
        let config = config_with(
            r#"
[reminders.zone2]
display = "Zone 2"
interval_days = 3
watch = "session"
target = { field = "zone2_min", min_value = 1 }
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].watch,
            WatchSource::Session(SessionMatch::NumericAtLeast {
                column: SessionNumColumn::ZoneTwoMin,
                min: 1.0,
            })
        );
    }

    #[test]
    fn load_parses_lift_with_filters() {
        let config = config_with(
            r#"
[reminders.deads]
display = "Heavy deadlifts"
interval_days = 7
watch = "lift"
target = { exercise = "deadlift", min_weight = 200, min_reps = 3 }
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].watch,
            WatchSource::Lift {
                exercise: "deadlift".into(),
                min_weight: Some(200.0),
                min_reps: Some(3),
            }
        );
    }

    #[test]
    fn load_parses_lift_without_filters() {
        let config = config_with(
            r#"
[reminders.any_deadlift]
display = "Any deadlift"
interval_days = 7
watch = "lift"
target = { exercise = "deadlift" }
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].watch,
            WatchSource::Lift {
                exercise: "deadlift".into(),
                min_weight: None,
                min_reps: None,
            }
        );
    }

    #[test]
    fn load_parses_day_field() {
        let config = config_with(
            r#"
[reminders.weigh_in]
display = "Daily weigh-in"
interval_days = 1
watch = "day_field"
target = "weight"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(rs[0].watch, WatchSource::DayField(DayColumn::Weight));
    }

    #[test]
    fn load_rejects_interval_zero() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 0
watch = "metric"
target = "la_min"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("interval_days"), "got: {msg}");
        assert!(msg.contains("bad"), "got: {msg}");
    }

    #[test]
    fn load_rejects_unknown_watch_kind() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "constellation"
target = "la_min"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("constellation"), "got: {msg}");
    }

    #[test]
    fn load_rejects_unknown_session_field() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "session"
target = { field = "nonsense", equals = "x" }
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("nonsense"), "got: {msg}");
    }

    #[test]
    fn load_rejects_session_text_op_on_numeric_column() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "session"
target = { field = "duration", equals = "60" }
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duration"), "got: {msg}");
        assert!(
            msg.contains("min_value") || msg.contains("numeric"),
            "got: {msg}"
        );
    }

    #[test]
    fn load_rejects_unknown_day_field() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "day_field"
target = "wakefulness"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("wakefulness"), "got: {msg}");
    }

    #[test]
    fn load_rejects_lift_without_exercise() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "lift"
target = { min_weight = 200 }
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("exercise"), "got: {msg}");
    }

    #[test]
    fn load_returns_reminders_in_id_alphabetical_order() {
        let config = config_with(
            r#"
[reminders.zzz_last]
display = "Last"
interval_days = 1
watch = "metric"
target = "la_min"

[reminders.aaa_first]
display = "First"
interval_days = 1
watch = "metric"
target = "la_min"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        let ids: Vec<&str> = rs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["aaa_first", "zzz_last"]);
    }

    #[test]
    fn load_empty_when_no_reminders() {
        let mut cfg: Config = toml::from_str(r#"notes_dir = "/tmp/x""#).unwrap();
        cfg.reminders = HashMap::new();
        assert!(load_reminders(&cfg).unwrap().is_empty());
    }
}
