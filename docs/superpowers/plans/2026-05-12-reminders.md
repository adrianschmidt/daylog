# Smart Reminders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `[reminders]` config block that watches existing DB tables and surfaces overdue reminders at the top of `vitalog today` and in the JSON of both `today` and `status`.

**Architecture:** A new `src/reminders.rs` (sibling to `goals.rs`) owns the types, config-to-domain conversion, and the evaluator. `today_cmd` and a newly extracted `status_cmd` both load reminders, call `reminders::evaluate`, and weave the results into their existing render paths. No new DB schema; one `MAX(date)` query per reminder.

**Tech Stack:** Rust, serde, rusqlite, chrono, color_eyre. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-05-12-reminders-design.md`

---

## File map

- **Create** `src/reminders.rs` — `Reminder`, `WatchSource`, `EvaluatedReminder`, `EvaluationResult`, `load_reminders`, `evaluate`. Pure helpers, in-memory-SQLite tests.
- **Create** `src/cli/status_cmd.rs` — `pub fn execute(config: &Config) -> Result<()>`. Body lifted from `main.rs::cmd_status`; the lift makes it testable (mirrors how `trend_cmd` was extracted).
- **Create** `tests/reminders.rs` — end-to-end integration tests over a tempdir config.
- **Modify** `src/config.rs` — add `reminders: HashMap<String, ReminderConfig>` to `Config` plus the `ReminderConfig`, `WatchKind`, and helper-target structs (all `#[derive(Deserialize)]`). Config-load validation only does shape/type checks; semantic checks (interval ≥ 1, whitelisted columns) happen in `reminders::load_reminders`.
- **Modify** `src/lib.rs` — `pub mod reminders;`.
- **Modify** `src/cli/mod.rs` — `pub mod status_cmd;`.
- **Modify** `src/cli/today_cmd.rs` — load + evaluate reminders inside `execute`, render the optional reminder block above the existing text output, add the `reminders` and `reminder_warnings` arrays to the JSON output. The existing `assemble`, `render_text`, and `render_json` signatures are kept intact; the reminder rendering is composed in `execute`.
- **Modify** `src/main.rs` — `cmd_status` becomes a thin wrapper over `vitalog::cli::status_cmd::execute`.
- **Modify** `presets/default.toml` — append a commented-out `[reminders.X]` example.
- **Modify** `README.md` — add a new tier or short subsection under "Three Tiers of Extensibility" (or its own H2 after that section) for reminders.
- **Modify** `CLAUDE.md` — add `src/reminders.rs` to the file map; mention `status_cmd.rs`; note the reminder hooks in `today_cmd.rs` and `main.rs`.

---

## Task 1: Scaffold types, module declaration, and empty Config field

**Files:**
- Create: `src/reminders.rs`
- Modify: `src/lib.rs`
- Modify: `src/config.rs`

Goal: get the new module compiling with public types and a stub `evaluate` returning empty. No behaviour yet, no tests yet — pure scaffolding so subsequent tasks have something to fill in.

- [ ] **Step 1: Create the empty module file**

Create `src/reminders.rs`:

```rust
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
pub fn load_reminders(_config: &Config) -> Result<Vec<Reminder>> {
    Ok(Vec::new())
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
```

- [ ] **Step 2: Declare the module**

In `src/lib.rs`, add `pub mod reminders;` so it sits alphabetically between `pub mod legacy;` and `pub mod materializer;`. The current file ends at line 16; insert the new line in alphabetical order.

The resulting block (replace the existing module declarations):

```rust
pub mod app;
pub mod body;
pub mod cli;
pub mod config;
pub mod db;
pub mod demo;
pub mod food_sum;
pub mod frontmatter;
pub mod goals;
pub mod legacy;
pub mod materializer;
pub mod modules;
pub mod reminders;
pub mod state;
pub mod template;
pub mod time;
```

- [ ] **Step 3: Add the empty `reminders` field on `Config`**

In `src/config.rs`, find the existing `pub struct Config { ... }` block (starts at line 76). Add a `reminders` field right after the existing `pub metrics: HashMap<String, MetricConfig>,` line:

```rust
    #[serde(default)]
    pub reminders: HashMap<String, ReminderConfig>,
```

Then, near the other config-element structs (after the `MetricConfig` definition at line ~149-156), add a placeholder `ReminderConfig` that we'll flesh out in Task 2:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ReminderConfig {
    pub display: String,
    pub interval_days: u32,
    pub watch: String,
    pub target: toml::Value,
    #[serde(default)]
    pub count_zero_as_logged: bool,
}
```

(`watch` is a `String` for now — Task 2 will tighten it to a `WatchKind` enum.)

- [ ] **Step 4: Verify it builds cleanly**

Run: `cargo build`
Expected: clean build, no warnings about unused fields (the new struct fields are public).

Run: `cargo test --lib reminders` (no tests yet, but ensures the module compiles in test mode too).
Expected: `0 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs src/lib.rs src/config.rs
git commit -m "$(cat <<'EOF'
feat(reminders): scaffold reminders module + Config field

Adds src/reminders.rs with the public type surface (Reminder,
WatchSource, EvaluatedReminder, EvaluationResult) and stub
load_reminders/evaluate functions. Adds an empty
Config.reminders field so [reminders] blocks parse but are not
yet acted on.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Config parsing — `ReminderConfig` → `Reminder`

**Files:**
- Modify: `src/config.rs`
- Modify: `src/reminders.rs`

Goal: turn TOML config blocks into validated `Reminder` values. Hard-fails on structural errors with `.suggestion()` messages.

- [ ] **Step 1: Write the failing test for valid configs**

In `src/reminders.rs`, add a `#[cfg(test)] mod tests` block at the bottom of the file:

```rust
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
        assert!(msg.contains("min_value") || msg.contains("numeric"), "got: {msg}");
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
```

- [ ] **Step 2: Run the tests and confirm they all fail**

Run: `cargo test --lib reminders::tests`
Expected: all the `load_parses_*` and `load_rejects_*` tests fail (the stub returns `Ok(vec![])`).

- [ ] **Step 3: Implement `load_reminders`**

Replace the stub `pub fn load_reminders(...)` in `src/reminders.rs` with:

```rust
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
                .suggestion(
                    "watch must be one of: metric, session, lift, day_field.",
                );
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
    let metric_id = target.as_str().ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "reminder `{id}`: watch = \"metric\" requires `target` to be a metric id string"
        )
    }).suggestion(r#"Example: target = "la_min""#)?;
    Ok(WatchSource::Metric {
        id: metric_id.to_string(),
        count_zero_as_logged,
    })
}

fn parse_session_watch(id: &str, target: &toml::Value) -> Result<WatchSource> {
    use color_eyre::Help;
    let table = target.as_table().ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "reminder `{id}`: watch = \"session\" requires `target` to be an inline table"
        )
    }).suggestion(
        r#"Example: target = { field = "zone2_min", min_value = 1 }"#,
    )?;
    let field = table.get("field").and_then(|v| v.as_str()).ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "reminder `{id}`: session target is missing `field`"
        )
    }).suggestion(
        r#"Example: target = { field = "zone2_min", min_value = 1 }"#,
    )?;

    let text_col = parse_session_text_column(field);
    let num_col = parse_session_numeric_column(field);
    let has_equals = table.contains_key("equals");
    let has_min_value = table.contains_key("min_value");

    match (text_col, num_col, has_equals, has_min_value) {
        (Some(col), _, true, false) => {
            let value = table.get("equals").and_then(|v| v.as_str()).ok_or_else(|| {
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
    let table = target.as_table().ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "reminder `{id}`: watch = \"lift\" requires `target` to be an inline table"
        )
    }).suggestion(r#"Example: target = { exercise = "deadlift", min_weight = 200 }"#)?;
    let exercise = table
        .get("exercise")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: lift target is missing `exercise`"
            )
        })
        .suggestion(r#"Example: target = { exercise = "deadlift" }"#)?;
    let min_weight = match table.get("min_weight") {
        None => None,
        Some(v) => Some(toml_value_as_f64(v).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "reminder `{id}`: lift target.min_weight must be a number"
            )
        })?),
    };
    let min_reps = match table.get("min_reps") {
        None => None,
        Some(v) => {
            let n = v.as_integer().ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "reminder `{id}`: lift target.min_reps must be an integer"
                )
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
    let name = target.as_str().ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "reminder `{id}`: watch = \"day_field\" requires `target` to be a string"
        )
    }).suggestion(r#"Example: target = "weight""#)?;
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
            .suggestion(
                "Allowed: weight, sleep_hours, mood, energy, sleep_start, sleep_end.",
            );
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
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test --lib reminders::tests`
Expected: all `load_*` tests pass. `evaluate` tests not yet added.

- [ ] **Step 5: Run the whole test suite for regressions**

Run: `cargo test`
Expected: everything passes. Config-parsing isn't yet used end-to-end so existing tests stay green.

- [ ] **Step 6: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): parse [reminders] config into Reminder values

Adds load_reminders, parsing TOML config blocks into typed
WatchSource variants with hard-fail validation on structural
errors (unknown watch kind, missing fields, wrong predicate
for column type). Reminders are returned sorted by id for
deterministic downstream rendering.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Evaluator — `Metric` watch

**Files:**
- Modify: `src/reminders.rs`

Goal: implement the SQL query and date math for `WatchSource::Metric`. Other watch kinds still go through the stub branch and return `due = true`.

- [ ] **Step 1: Add test infrastructure (helpers + first failing test)**

In `src/reminders.rs`'s `mod tests`, after the existing tests, append:

```rust
    use rusqlite::Connection;

    /// Minimal in-memory DB with the tables `evaluate` reads.
    /// We mirror the production schema shape but skip foreign keys to keep
    /// the test setup compact — these tests only exercise the reminder
    /// queries, not referential integrity.
    fn make_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE days (
                date TEXT PRIMARY KEY,
                sleep_start TEXT,
                sleep_end TEXT,
                sleep_hours REAL,
                mood INTEGER,
                energy INTEGER,
                weight REAL
            );
            CREATE TABLE metrics (
                date TEXT NOT NULL,
                name TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (date, name)
            );
            CREATE TABLE sessions (
                date TEXT NOT NULL,
                session_number INTEGER NOT NULL DEFAULT 1,
                session_type TEXT,
                week INTEGER,
                block TEXT,
                duration INTEGER,
                rpe REAL,
                zone2_min INTEGER,
                hr_avg INTEGER,
                vo2_intervals TEXT,
                PRIMARY KEY (date, session_number)
            );
            CREATE TABLE lift_sets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                session_number INTEGER NOT NULL DEFAULT 1,
                exercise TEXT NOT NULL,
                set_number INTEGER NOT NULL,
                weight_lbs REAL NOT NULL,
                reps INTEGER NOT NULL,
                estimated_1rm REAL
            );
            ",
        )
        .unwrap();
        conn
    }

    fn metric_reminder(id: &str, interval_days: u32, metric: &str) -> Reminder {
        Reminder {
            id: id.into(),
            display: id.into(),
            interval_days,
            watch: WatchSource::Metric {
                id: metric.into(),
                count_zero_as_logged: false,
            },
        }
    }

    fn insert_day(conn: &Connection, date: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO days(date) VALUES (?1)",
            [date],
        )
        .unwrap();
    }

    fn insert_metric(conn: &Connection, date: &str, name: &str, value: f64) {
        insert_day(conn, date);
        conn.execute(
            "INSERT INTO metrics(date, name, value) VALUES (?1, ?2, ?3)",
            rusqlite::params![date, name, value],
        )
        .unwrap();
    }

    fn empty_config() -> Config {
        toml::from_str(
            r#"
notes_dir = "/tmp/x"

[metrics]
la_min = { display = "LA", color = "red" }
"#,
        )
        .unwrap()
    }

    #[test]
    fn evaluate_metric_never_logged_is_due() {
        let conn = make_test_db();
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
        assert_eq!(result.reminders.len(), 1);
        let r = &result.reminders[0];
        assert_eq!(r.last_done, None);
        assert_eq!(r.days_since, None);
        assert!(r.due);
    }

    #[test]
    fn evaluate_metric_logged_today_not_due() {
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-12", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
        let r = &result.reminders[0];
        assert_eq!(r.last_done, Some(today));
        assert_eq!(r.days_since, Some(0));
        assert!(!r.due);
    }

    #[test]
    fn evaluate_metric_logged_exactly_interval_days_ago_is_due() {
        // interval=2, logged Monday at 23:00 (DB only stores date),
        // checking Wednesday → due all day Wednesday.
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-10", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
        let r = &result.reminders[0];
        assert_eq!(r.last_done, Some(NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()));
        assert_eq!(r.days_since, Some(2));
        assert!(r.due);
    }

    #[test]
    fn evaluate_metric_logged_interval_minus_one_days_ago_not_due() {
        // interval=2, logged yesterday → not due today.
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-11", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
        let r = &result.reminders[0];
        assert_eq!(r.days_since, Some(1));
        assert!(!r.due);
    }

    #[test]
    fn evaluate_metric_zero_value_does_not_count_by_default() {
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-12", "la_min", 0.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_metric_zero_value_counts_when_opted_in() {
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-12", "la_min", 0.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let reminder = Reminder {
            id: "la".into(),
            display: "LA".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: true,
            },
        };
        let result = evaluate(&conn, today, &[reminder], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, Some(today));
        assert!(!result.reminders[0].due);
    }
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test --lib reminders::tests::evaluate_metric`
Expected: all `evaluate_metric_*` tests fail (stub always returns `due = true, last_done = None`).

- [ ] **Step 3: Replace the stub `evaluate` with the real implementation**

In `src/reminders.rs`, replace the existing `pub fn evaluate(...)` body with:

```rust
pub fn evaluate(
    conn: &Connection,
    today: NaiveDate,
    reminders: &[Reminder],
    config: &Config,
) -> Result<EvaluationResult> {
    let mut out = Vec::with_capacity(reminders.len());
    let mut warnings = Vec::new();
    for r in reminders {
        let last_done = query_last_done(conn, &r.watch)?;
        if let WatchSource::Metric { id, .. } = &r.watch {
            if last_done.is_none() && !config.metrics.contains_key(id) {
                warnings.push(format!(
                    "reminder `{}`: target metric `{id}` is not declared in [metrics]",
                    r.id
                ));
            }
        }
        let days_since = last_done.map(|d| (today - d).num_days());
        let due = match days_since {
            None => true,
            Some(n) => n >= r.interval_days as i64,
        };
        out.push(EvaluatedReminder {
            id: r.id.clone(),
            display: r.display.clone(),
            interval_days: r.interval_days,
            last_done,
            days_since,
            due,
        });
    }
    Ok(EvaluationResult {
        reminders: out,
        warnings,
    })
}

/// Run one MAX(date) query per watch kind. Column names are taken from
/// the closed enum variants — never substituted from user input — so
/// `format!`-ing them into the SQL is safe.
fn query_last_done(conn: &Connection, watch: &WatchSource) -> Result<Option<NaiveDate>> {
    use color_eyre::eyre::WrapErr;

    let date_str: Option<String> = match watch {
        WatchSource::Metric {
            id,
            count_zero_as_logged,
        } => {
            let zero_flag: i64 = if *count_zero_as_logged { 1 } else { 0 };
            conn.query_row(
                "SELECT MAX(date) FROM metrics WHERE name = ?1 AND (value > 0 OR ?2 = 1)",
                rusqlite::params![id, zero_flag],
                |row| row.get::<_, Option<String>>(0),
            )
            .wrap_err("Failed to query metrics for reminder")?
        }
        // Other variants are filled in by later tasks; for now they all
        // return None so we don't break the test scaffold.
        _ => None,
    };
    Ok(date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()))
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test --lib reminders::tests`
Expected: all `load_*` and `evaluate_metric_*` tests pass. Old `evaluate` stub-test (if any) — there shouldn't be one, since Task 1 didn't add tests.

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): evaluate metric watches against the DB

Queries `MAX(date) FROM metrics WHERE name = ?1 AND (value > 0
OR ?2 = 1)` per metric reminder and computes calendar-day
gap to `today`. Honours `count_zero_as_logged`. Emits a soft
warning when the target metric is not declared in [metrics],
mirroring goals.md's "unknown metric" hint.

Other watch kinds remain stubs returning None — filled in by
follow-up commits.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Evaluator — `Session` watch

**Files:**
- Modify: `src/reminders.rs`

Goal: implement both `TextEquals` and `NumericAtLeast` branches against the `sessions` table.

- [ ] **Step 1: Add failing tests**

In `src/reminders.rs::tests`, append:

```rust
    fn insert_session(
        conn: &Connection,
        date: &str,
        session_type: Option<&str>,
        zone2_min: Option<i64>,
    ) {
        insert_day(conn, date);
        conn.execute(
            "INSERT INTO sessions(date, session_number, session_type, zone2_min) \
             VALUES (?1, 1, ?2, ?3)",
            rusqlite::params![date, session_type, zone2_min],
        )
        .unwrap();
    }

    fn session_text_reminder(id: &str, interval: u32, column: SessionTextColumn, value: &str) -> Reminder {
        Reminder {
            id: id.into(),
            display: id.into(),
            interval_days: interval,
            watch: WatchSource::Session(SessionMatch::TextEquals {
                column,
                value: value.into(),
            }),
        }
    }

    fn session_num_reminder(id: &str, interval: u32, column: SessionNumColumn, min: f64) -> Reminder {
        Reminder {
            id: id.into(),
            display: id.into(),
            interval_days: interval,
            watch: WatchSource::Session(SessionMatch::NumericAtLeast { column, min }),
        }
    }

    #[test]
    fn evaluate_session_text_equals_match() {
        let conn = make_test_db();
        insert_session(&conn, "2026-05-10", Some("vo2_max"), None);
        insert_session(&conn, "2026-05-11", Some("zone2"), None);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = session_text_reminder("vo2", 7, SessionTextColumn::Type, "vo2_max");
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 10).unwrap())
        );
        assert!(!result.reminders[0].due);
    }

    #[test]
    fn evaluate_session_text_equals_no_match_is_due() {
        let conn = make_test_db();
        insert_session(&conn, "2026-05-10", Some("zone2"), None);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = session_text_reminder("vo2", 7, SessionTextColumn::Type, "vo2_max");
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_session_numeric_at_least_match() {
        let conn = make_test_db();
        insert_session(&conn, "2026-05-09", None, Some(0));
        insert_session(&conn, "2026-05-10", None, Some(30));
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = session_num_reminder("zone2", 3, SessionNumColumn::ZoneTwoMin, 1.0);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 10).unwrap())
        );
        assert!(!result.reminders[0].due);
    }

    #[test]
    fn evaluate_session_numeric_below_threshold_not_counted() {
        let conn = make_test_db();
        insert_session(&conn, "2026-05-11", None, Some(15));
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = session_num_reminder("zone2_long", 1, SessionNumColumn::ZoneTwoMin, 30.0);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(result.reminders[0].due);
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib reminders::tests::evaluate_session`
Expected: all four `evaluate_session_*` tests fail (the `_ => None` fall-through returns None).

- [ ] **Step 3: Fill in the `Session` branches**

In `src/reminders.rs`, replace the `_ => None,` line in `query_last_done` with explicit `Session` handling and a new fall-through. The function becomes:

```rust
fn query_last_done(conn: &Connection, watch: &WatchSource) -> Result<Option<NaiveDate>> {
    use color_eyre::eyre::WrapErr;

    let date_str: Option<String> = match watch {
        WatchSource::Metric {
            id,
            count_zero_as_logged,
        } => {
            let zero_flag: i64 = if *count_zero_as_logged { 1 } else { 0 };
            conn.query_row(
                "SELECT MAX(date) FROM metrics WHERE name = ?1 AND (value > 0 OR ?2 = 1)",
                rusqlite::params![id, zero_flag],
                |row| row.get::<_, Option<String>>(0),
            )
            .wrap_err("Failed to query metrics for reminder")?
        }
        WatchSource::Session(SessionMatch::TextEquals { column, value }) => {
            let sql = format!(
                "SELECT MAX(date) FROM sessions WHERE {} = ?1",
                column.sql_column()
            );
            conn.query_row(&sql, [value], |row| row.get::<_, Option<String>>(0))
                .wrap_err("Failed to query sessions for reminder (text-equals)")?
        }
        WatchSource::Session(SessionMatch::NumericAtLeast { column, min }) => {
            let sql = format!(
                "SELECT MAX(date) FROM sessions WHERE {col} IS NOT NULL AND {col} >= ?1",
                col = column.sql_column()
            );
            conn.query_row(&sql, rusqlite::params![min], |row| {
                row.get::<_, Option<String>>(0)
            })
            .wrap_err("Failed to query sessions for reminder (numeric-at-least)")?
        }
        _ => None,
    };
    Ok(date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()))
}
```

- [ ] **Step 4: Run and confirm they pass**

Run: `cargo test --lib reminders::tests`
Expected: all existing tests pass, including the four new `evaluate_session_*` ones.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): evaluate session watches

Adds the two session-watch branches: TextEquals (e.g.
session_type = "vo2_max") and NumericAtLeast (e.g.
zone2_min >= 30). Column names come from closed enum variants,
not user strings.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Evaluator — `Lift` watch

**Files:**
- Modify: `src/reminders.rs`

- [ ] **Step 1: Add failing tests**

In `src/reminders.rs::tests`, append:

```rust
    fn insert_lift(
        conn: &Connection,
        date: &str,
        exercise: &str,
        weight_lbs: f64,
        reps: i64,
    ) {
        insert_day(conn, date);
        conn.execute(
            "INSERT INTO sessions(date, session_number) VALUES (?1, 1) \
             ON CONFLICT(date, session_number) DO NOTHING",
            [date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO lift_sets(date, session_number, exercise, set_number, weight_lbs, reps) \
             VALUES (?1, 1, ?2, 1, ?3, ?4)",
            rusqlite::params![date, exercise, weight_lbs, reps],
        )
        .unwrap();
    }

    fn lift_reminder(
        id: &str,
        interval: u32,
        exercise: &str,
        min_weight: Option<f64>,
        min_reps: Option<u32>,
    ) -> Reminder {
        Reminder {
            id: id.into(),
            display: id.into(),
            interval_days: interval,
            watch: WatchSource::Lift {
                exercise: exercise.into(),
                min_weight,
                min_reps,
            },
        }
    }

    #[test]
    fn evaluate_lift_exercise_only() {
        let conn = make_test_db();
        insert_lift(&conn, "2026-05-09", "deadlift", 200.0, 5);
        insert_lift(&conn, "2026-05-11", "squat", 185.0, 5);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = lift_reminder("deads", 3, "deadlift", None, None);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap())
        );
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_lift_with_min_weight_excludes_lighter_sets() {
        let conn = make_test_db();
        insert_lift(&conn, "2026-05-09", "deadlift", 200.0, 5);
        insert_lift(&conn, "2026-05-11", "deadlift", 135.0, 5);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = lift_reminder("heavy_deads", 7, "deadlift", Some(180.0), None);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap())
        );
    }

    #[test]
    fn evaluate_lift_with_min_reps_excludes_shorter_sets() {
        let conn = make_test_db();
        insert_lift(&conn, "2026-05-09", "deadlift", 200.0, 5);
        insert_lift(&conn, "2026-05-11", "deadlift", 250.0, 2);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = lift_reminder("deads_for_reps", 7, "deadlift", None, Some(4));
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap())
        );
    }

    #[test]
    fn evaluate_lift_no_match_is_due() {
        let conn = make_test_db();
        insert_lift(&conn, "2026-05-09", "squat", 185.0, 5);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = lift_reminder("deads", 3, "deadlift", None, None);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(result.reminders[0].due);
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib reminders::tests::evaluate_lift`
Expected: all four fail.

- [ ] **Step 3: Add the `Lift` branch**

In `query_last_done` (above the `_ => None,` fall-through), add:

```rust
        WatchSource::Lift {
            exercise,
            min_weight,
            min_reps,
        } => {
            let mut sql = String::from("SELECT MAX(date) FROM lift_sets WHERE exercise = ?1");
            // We bind params in order; build the SQL conditionally to match.
            let mut params: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Text(exercise.clone())];
            if let Some(w) = min_weight {
                sql.push_str(&format!(" AND weight_lbs >= ?{}", params.len() + 1));
                params.push(rusqlite::types::Value::Real(*w));
            }
            if let Some(r) = min_reps {
                sql.push_str(&format!(" AND reps >= ?{}", params.len() + 1));
                params.push(rusqlite::types::Value::Integer(*r as i64));
            }
            let params_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            conn.query_row(&sql, params_refs.as_slice(), |row| {
                row.get::<_, Option<String>>(0)
            })
            .wrap_err("Failed to query lift_sets for reminder")?
        }
```

- [ ] **Step 4: Run and confirm they pass**

Run: `cargo test --lib reminders::tests`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): evaluate lift watches with optional min filters

Adds the Lift branch — matches on exercise name, with optional
min_weight (lbs) and min_reps narrowing which lift_sets rows
count. Builds the WHERE clause incrementally to keep the SQL
honest about which params are bound.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Evaluator — `DayField` watch + unknown-metric warning

**Files:**
- Modify: `src/reminders.rs`

Goal: finish the four watch kinds, and add an explicit test for the unknown-metric-target warning path.

- [ ] **Step 1: Add failing tests**

In `src/reminders.rs::tests`, append:

```rust
    fn insert_day_field(conn: &Connection, date: &str, column: &str, value: &str) {
        insert_day(conn, date);
        let sql = format!("UPDATE days SET {column} = ?1 WHERE date = ?2");
        conn.execute(&sql, [value, date]).unwrap();
    }

    fn day_field_reminder(id: &str, interval: u32, col: DayColumn) -> Reminder {
        Reminder {
            id: id.into(),
            display: id.into(),
            interval_days: interval,
            watch: WatchSource::DayField(col),
        }
    }

    #[test]
    fn evaluate_day_field_weight_match() {
        let conn = make_test_db();
        insert_day_field(&conn, "2026-05-11", "weight", "121.5");
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = day_field_reminder("weigh_in", 1, DayColumn::Weight);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(
            result.reminders[0].last_done,
            Some(NaiveDate::from_ymd_opt(2026, 5, 11).unwrap())
        );
        assert_eq!(result.reminders[0].days_since, Some(1));
        // interval=1, days_since=1 → due (1 >= 1)
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_day_field_sleep_hours_match() {
        let conn = make_test_db();
        insert_day_field(&conn, "2026-05-12", "sleep_hours", "7.5");
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = day_field_reminder("sleep_log", 1, DayColumn::SleepHours);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].days_since, Some(0));
        assert!(!result.reminders[0].due);
    }

    #[test]
    fn evaluate_day_field_no_rows_is_due() {
        let conn = make_test_db();
        // A day row exists but `weight` is null.
        insert_day(&conn, "2026-05-12");
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = day_field_reminder("weigh_in", 1, DayColumn::Weight);
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_unknown_metric_target_emits_warning() {
        let conn = make_test_db();
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = metric_reminder("typoed", 1, "definitely_not_in_metrics");
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert!(result.reminders[0].due);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("definitely_not_in_metrics") && w.contains("typoed")),
            "expected unknown-metric warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn evaluate_unknown_metric_with_history_does_not_warn() {
        // If the metric isn't in [metrics] but the DB has historical rows,
        // suppress the warning — the user is likely watching legacy data.
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-05", "legacy_metric", 1.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = metric_reminder("legacy", 1, "legacy_metric");
        let result = evaluate(&conn, today, &[r], &empty_config()).unwrap();
        assert!(result.warnings.is_empty(), "got: {:?}", result.warnings);
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib reminders::tests::evaluate_day_field` and
`cargo test --lib reminders::tests::evaluate_unknown_metric`
Expected: all five tests fail. (Day-field branch is stubbed; the
unknown-metric-with-history test currently passes because we only
warn when `last_done` is `None`, but the explicit DayField tests
need the new branch.)

- [ ] **Step 3: Add the `DayField` branch**

In `query_last_done`, above the `_ => None,` fall-through, add:

```rust
        WatchSource::DayField(col) => {
            let sql = format!(
                "SELECT MAX(date) FROM days WHERE {} IS NOT NULL",
                col.sql_column()
            );
            conn.query_row(&sql, [], |row| row.get::<_, Option<String>>(0))
                .wrap_err("Failed to query days for reminder")?
        }
```

After this, the `_ => None,` fall-through is unreachable (all variants are now matched). Remove it; the compiler will enforce exhaustiveness going forward.

- [ ] **Step 4: Run and confirm passes**

Run: `cargo test --lib reminders::tests`
Expected: all green.

Run: `cargo test`
Expected: clean overall.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): evaluate day_field watches; exhaustive match

Adds the DayField branch (any non-null value in the named days
column counts as "logged") and drops the catch-all arm — the
match is now exhaustive over WatchSource, which will surface
new variants as compile errors if they're added later.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `today_cmd` — render the reminder block above the summary

**Files:**
- Modify: `src/cli/today_cmd.rs`

Goal: wire reminder loading + evaluation into `today_cmd::execute`, and add a `render_reminders_block` helper that prepends the due reminders. Keep `render_text` and `render_json` signatures backwards-compatible; compose the new output in `execute`.

- [ ] **Step 1: Add failing tests for the new helper**

At the bottom of `src/cli/today_cmd.rs::tests`, append:

```rust
    use crate::reminders::EvaluatedReminder;

    fn evald(id: &str, display: &str, days_since: Option<i64>, last_done: Option<NaiveDate>, due: bool, interval: u32) -> EvaluatedReminder {
        EvaluatedReminder {
            id: id.into(),
            display: display.into(),
            interval_days: interval,
            last_done,
            days_since,
            due,
        }
    }

    #[test]
    fn reminders_block_empty_when_nothing_due() {
        let rs = vec![evald("a", "A", Some(0), Some(NaiveDate::from_ymd_opt(2026, 5, 12).unwrap()), false, 1)];
        let block = render_reminders_block(&rs, false);
        assert_eq!(block, "");
    }

    #[test]
    fn reminders_block_empty_when_no_reminders() {
        let block = render_reminders_block(&[], false);
        assert_eq!(block, "");
    }

    #[test]
    fn reminders_block_renders_due_lines_with_days_since() {
        let rs = vec![
            evald(
                "lactic_acid",
                "Lactic acid training",
                Some(3),
                Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()),
                true,
                2,
            ),
            evald(
                "weigh_in",
                "Daily weigh-in",
                None,
                None,
                true,
                1,
            ),
        ];
        let block = render_reminders_block(&rs, false);
        assert!(block.contains("Reminders"), "got:\n{block}");
        assert!(block.contains("Lactic acid training"), "got:\n{block}");
        assert!(block.contains("3 days ago"), "got:\n{block}");
        assert!(block.contains("2026-05-09"), "got:\n{block}");
        assert!(block.contains("Daily weigh-in"), "got:\n{block}");
        assert!(block.contains("never logged"), "got:\n{block}");
        // Block ends with a blank line separator before the date header.
        assert!(block.ends_with("\n\n"), "got:\n{block:?}");
    }

    #[test]
    fn reminders_block_orders_most_overdue_first() {
        let rs = vec![
            evald("a", "A two-day", Some(2), Some(NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()), true, 1),
            evald("b", "Never B", None, None, true, 1),
            evald("c", "C five-day", Some(5), Some(NaiveDate::from_ymd_opt(2026, 5, 7).unwrap()), true, 1),
        ];
        let block = render_reminders_block(&rs, false);
        let lines: Vec<&str> = block.lines().filter(|l| l.starts_with("- ")).collect();
        // never-logged ranks above any finite days_since; then descending
        // by days_since.
        assert!(lines[0].contains("Never B"), "got:\n{block}");
        assert!(lines[1].contains("C five-day"), "got:\n{block}");
        assert!(lines[2].contains("A two-day"), "got:\n{block}");
    }

    #[test]
    fn reminders_block_skips_not_due_entries() {
        let rs = vec![
            evald("due", "Due one", Some(2), Some(NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()), true, 1),
            evald("ok", "Not due one", Some(0), Some(NaiveDate::from_ymd_opt(2026, 5, 12).unwrap()), false, 1),
        ];
        let block = render_reminders_block(&rs, false);
        assert!(block.contains("Due one"), "got:\n{block}");
        assert!(!block.contains("Not due one"), "got:\n{block}");
    }

    #[test]
    fn reminders_block_color_on_emits_ansi() {
        let rs = vec![evald("a", "A", Some(3), Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()), true, 2)];
        let block = render_reminders_block(&rs, true);
        assert!(block.contains("\x1b["), "expected ANSI codes, got:\n{block:?}");
    }

    #[test]
    fn reminders_block_color_off_strips_ansi() {
        let rs = vec![evald("a", "A", Some(3), Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()), true, 2)];
        let block = render_reminders_block(&rs, false);
        assert!(!block.contains("\x1b["), "got:\n{block:?}");
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib today_cmd::tests::reminders_block`
Expected: compile failure (function does not exist).

- [ ] **Step 3: Add the `render_reminders_block` function**

In `src/cli/today_cmd.rs`, add this new function near `render_text` (just above is fine):

```rust
/// Render the "Reminders" block to prepend above the daily summary.
/// Returns `""` when no reminder is due — caller can append unconditionally.
///
/// Ordering: never-logged first, then by `days_since` descending (most
/// overdue first). Stable for equal keys via the input order, which
/// `reminders::load_reminders` already sorts alphabetically by id.
pub fn render_reminders_block(reminders: &[crate::reminders::EvaluatedReminder], color: bool) -> String {
    let mut due: Vec<&crate::reminders::EvaluatedReminder> =
        reminders.iter().filter(|r| r.due).collect();
    if due.is_empty() {
        return String::new();
    }
    due.sort_by(|a, b| match (a.days_since, b.days_since) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => y.cmp(&x),
    });

    let mut out = String::new();
    let header = paint(color, RED, "⏰ Reminders");
    out.push_str(&header);
    out.push('\n');
    for r in due {
        let line = match (r.days_since, r.last_done) {
            (Some(n), Some(d)) => {
                let plural = if n == 1 { "" } else { "s" };
                format!(
                    "- {} — overdue ({n} day{plural} ago, {})",
                    r.display,
                    d.format("%Y-%m-%d")
                )
            }
            _ => format!("- {} — never logged", r.display),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');
    out
}
```

(`paint`, `RED`, and `RESET` already exist in `today_cmd.rs`; reuse them.)

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib today_cmd::tests::reminders_block`
Expected: all six tests pass.

- [ ] **Step 5: Wire `execute` to actually load + evaluate + render**

In `src/cli/today_cmd.rs::execute`, after the existing `let mut summary = build_summary(date, config)?;` line and the warning-collection block, but before the `if json { ... } else { ... }` branch, add:

```rust
    let reminders_defs = crate::reminders::load_reminders(config)?;
    let reminder_eval = if reminders_defs.is_empty() {
        crate::reminders::EvaluationResult::default()
    } else {
        let conn = crate::db::open_ro(&config.db_path())?;
        crate::reminders::evaluate(&conn, date, &reminders_defs, config)?
    };
    for w in &reminder_eval.warnings {
        summary.goals_warnings.push(w.clone());
    }
```

Then in the text branch (`} else { ... print!(...) }`), prepend the reminder block:

```rust
    } else {
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        print!("{}", render_reminders_block(&reminder_eval.reminders, color));
        print!("{}", render_text(&summary, &goals, color));
    }
```

(Note: `build_summary` already opens an `rw` conn and syncs. We re-open a separate `ro` conn here to evaluate reminders; this avoids threading the connection out of `build_summary`. SQLite WAL handles concurrent readers without contention.)

- [ ] **Step 6: Add an `execute`-level test that exercises the wiring**

Append to `src/cli/today_cmd.rs::tests`:

```rust
    #[test]
    fn execute_text_prepends_reminders_block_when_due() {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_str = format!(
            r#"
notes_dir = "{}"
time_format = "24h"
weight_unit = "kg"

[metrics]
la_min = {{ display = "Lactic acid (min)", color = "red" }}

[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
            dir.path().display().to_string().replace('\\', "/")
        );
        let config: Config = toml::from_str(&toml_str).unwrap();

        let registry = crate::modules::build_registry(&config);
        let conn = db::open_rw(&config.db_path()).unwrap();
        db::init_db(&conn, &registry).unwrap();
        crate::modules::validate_module_tables(&registry).unwrap();
        // Seed nothing — la_min has never been logged → reminder is due.

        // Smoke: execute should not error and the rendered text (captured
        // via the pure helper) should contain the reminder line above
        // the date header.
        let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let goals = crate::goals::load_goals(&config.notes_dir_path()).unwrap();
        let summary = assemble(date, &config, &conn).unwrap();
        let reminders = crate::reminders::load_reminders(&config).unwrap();
        let eval = crate::reminders::evaluate(&conn, date, &reminders, &config).unwrap();

        let mut out = render_reminders_block(&eval.reminders, false);
        out.push_str(&render_text(&summary, &goals, false));
        let header_idx = out.find("2026-05-12 — Daily summary").expect("header present");
        let block_idx = out.find("Lactic acid training").expect("reminder present");
        assert!(block_idx < header_idx, "reminder block must precede summary; got:\n{out}");
    }
```

- [ ] **Step 7: Run and confirm**

Run: `cargo test --lib today_cmd`
Expected: all tests pass.

- [ ] **Step 8: Manual smoke test (optional but recommended)**

The smoke test must use a far-future date per `vitalog-workspace/CLAUDE.md` to avoid clobbering Adrian's real notes.

```bash
# Pre-condition: a [reminders.lactic_acid] block already in your
# real ~/.config/vitalog/config.toml referring to a metric you have
# not logged at the test date.
cargo run -- today 2099-01-01 | head -10
# Expected: prints a "⏰ Reminders" header line followed by a "- ...
# — never logged" line, then the date header.
```

If you added a real `[reminders.X]` block to your config for this smoke test, leave it in place (it's the actual feature use case); otherwise remove it.

- [ ] **Step 9: Commit**

```bash
git add src/cli/today_cmd.rs
git commit -m "$(cat <<'EOF'
feat(reminders): prepend due reminders block in `vitalog today` text

Adds render_reminders_block (most-overdue-first ordering,
"never logged" line for missing data, red header) and wires
load_reminders + evaluate into today_cmd::execute. The block
is suppressed entirely when nothing is due, so silent days
stay clean. Reminder warnings flow into the existing hints
block via summary.goals_warnings.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `today_cmd` — JSON output with `reminders` + `reminder_warnings`

**Files:**
- Modify: `src/cli/today_cmd.rs`

Goal: extend the JSON output with the always-present `reminders` array and a `reminder_warnings` sibling (distinct from the existing `warnings` array, per the spec).

- [ ] **Step 1: Add failing tests**

Append to `src/cli/today_cmd.rs::tests`:

```rust
    #[test]
    fn render_json_includes_empty_reminders_when_none_configured() {
        let s = fixture_summary();
        let g = fixture_goals();
        let v = render_json_with_reminders(&s, &g, &[], &[]);
        assert!(v["reminders"].is_array(), "got:\n{v}");
        assert_eq!(v["reminders"].as_array().unwrap().len(), 0);
        assert!(v["reminder_warnings"].is_array(), "got:\n{v}");
        assert_eq!(v["reminder_warnings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn render_json_lists_all_reminders_including_not_due() {
        let s = fixture_summary();
        let g = fixture_goals();
        let rs = vec![
            evald(
                "lactic_acid",
                "Lactic acid training",
                Some(3),
                Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()),
                true,
                2,
            ),
            evald(
                "weigh_in",
                "Daily weigh-in",
                Some(0),
                Some(NaiveDate::from_ymd_opt(2026, 5, 12).unwrap()),
                false,
                1,
            ),
        ];
        let v = render_json_with_reminders(&s, &g, &rs, &[]);
        let arr = v["reminders"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let la = &arr[0];
        assert_eq!(la["id"], "lactic_acid");
        assert_eq!(la["display"], "Lactic acid training");
        assert_eq!(la["interval_days"], 2);
        assert_eq!(la["last_done"], "2026-05-09");
        assert_eq!(la["days_since"], 3);
        assert_eq!(la["due"], true);

        let weigh = &arr[1];
        assert_eq!(weigh["id"], "weigh_in");
        assert_eq!(weigh["due"], false);
        assert_eq!(weigh["days_since"], 0);
    }

    #[test]
    fn render_json_reminder_with_no_last_done_uses_null() {
        let s = fixture_summary();
        let g = fixture_goals();
        let rs = vec![evald("never", "Never logged", None, None, true, 1)];
        let v = render_json_with_reminders(&s, &g, &rs, &[]);
        let r = &v["reminders"][0];
        assert!(r["last_done"].is_null());
        assert!(r["days_since"].is_null());
    }

    #[test]
    fn render_json_includes_reminder_warnings() {
        let s = fixture_summary();
        let g = fixture_goals();
        let v = render_json_with_reminders(&s, &g, &[], &["reminder `x`: target metric `y` is not declared in [metrics]".to_string()]);
        let w = v["reminder_warnings"].as_array().unwrap();
        assert_eq!(w.len(), 1);
        assert!(w[0].as_str().unwrap().contains("target metric"));
        // The regular `warnings` array stays untouched.
        let regular = v["warnings"].as_array().unwrap();
        assert!(regular.iter().all(|x| !x.as_str().unwrap().contains("target metric")));
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib today_cmd::tests::render_json`
Expected: compile error — `render_json_with_reminders` doesn't exist.

- [ ] **Step 3: Add `render_json_with_reminders` and wire it through `execute`**

In `src/cli/today_cmd.rs`, after the existing `render_json` definition, add:

```rust
/// Like `render_json` but also embeds the `reminders` array and a
/// `reminder_warnings` sibling. The existing `warnings` array is left
/// untouched — reminder warnings stay in their own stream so JSON
/// consumers can route them separately (per spec).
pub fn render_json_with_reminders(
    summary: &DaySummary,
    goals: &Goals,
    reminders: &[crate::reminders::EvaluatedReminder],
    reminder_warnings: &[String],
) -> serde_json::Value {
    let mut v = render_json(summary, goals);

    let rs_json: Vec<serde_json::Value> = reminders
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "display": r.display,
                "interval_days": r.interval_days,
                "last_done": r.last_done.map(|d| d.format("%Y-%m-%d").to_string()),
                "days_since": r.days_since,
                "due": r.due,
            })
        })
        .collect();
    v["reminders"] = serde_json::Value::Array(rs_json);

    let warns_json: Vec<serde_json::Value> = reminder_warnings
        .iter()
        .cloned()
        .map(serde_json::Value::String)
        .collect();
    v["reminder_warnings"] = serde_json::Value::Array(warns_json);

    v
}
```

Now update `execute` to call this new helper in the JSON branch. Replace the existing JSON branch:

```rust
    if json {
        let v = render_json(&summary, &goals);
        println!("{}", serde_json::to_string_pretty(&v)?);
    }
```

with:

```rust
    if json {
        let v = render_json_with_reminders(
            &summary,
            &goals,
            &reminder_eval.reminders,
            &reminder_eval.warnings,
        );
        println!("{}", serde_json::to_string_pretty(&v)?);
    }
```

Also remove the line added in Task 7 that pushed reminder warnings into `summary.goals_warnings`. Reminder warnings should NOT pollute the goals warnings — they go into their own stream in both text and JSON.

Actually, in the text path we DO want them to render somewhere. Replace the previously-added line with a separate render pass after the existing hints block. Adjust `execute` so the structure is:

```rust
    let reminders_defs = crate::reminders::load_reminders(config)?;
    let reminder_eval = if reminders_defs.is_empty() {
        crate::reminders::EvaluationResult::default()
    } else {
        let conn = crate::db::open_ro(&config.db_path())?;
        crate::reminders::evaluate(&conn, date, &reminders_defs, config)?
    };

    if json {
        let v = render_json_with_reminders(
            &summary,
            &goals,
            &reminder_eval.reminders,
            &reminder_eval.warnings,
        );
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        print!("{}", render_reminders_block(&reminder_eval.reminders, color));
        print!("{}", render_text(&summary, &goals, color));
        for w in &reminder_eval.warnings {
            let line = paint(color, DIM, &format!("({w})"));
            println!("{line}");
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Update the test added in Task 7 to reflect the new behaviour**

The test `execute_text_prepends_reminders_block_when_due` was written to use `render_reminders_block` + `render_text`, which we haven't changed. It still passes. No change needed.

- [ ] **Step 5: Run and confirm**

Run: `cargo test --lib today_cmd`
Expected: all green.

Run: `cargo test`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/cli/today_cmd.rs
git commit -m "$(cat <<'EOF'
feat(reminders): reminders array + reminder_warnings in today --json

Adds render_json_with_reminders, called from today_cmd::execute
in the JSON branch. The `reminders` array always lists every
configured reminder (with `last_done`/`days_since`/`due`); a
sibling `reminder_warnings` array carries soft warnings,
distinct from the existing `warnings` stream (which keeps its
existing goals/food-parse contents).

Reminder warnings also render in the dim hint block in the text
output, mirroring how goals warnings are surfaced today.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Extract `cmd_status` into `cli/status_cmd.rs` (refactor)

**Files:**
- Create: `src/cli/status_cmd.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

Goal: make the status command testable from a unit-test context (mirrors the trend command refactor). Pure refactor in this task — no reminders yet. Behaviour-preserving.

- [ ] **Step 1: Create the new file with `execute` containing the current logic**

Create `src/cli/status_cmd.rs`:

```rust
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
```

Note: changed `open_ro` → `open_rw` + `init_db` + `sync_all` to match `today_cmd`'s pre-read sync (per spec). Sync errors are intentionally swallowed via `let _ = ...`.

- [ ] **Step 2: Declare the module**

In `src/cli/mod.rs`, add `pub mod status_cmd;` alongside the other CLI submodules (alphabetical: between `pub mod sleep_cmd;` and `pub mod today_cmd;`).

- [ ] **Step 3: Slim down `main.rs::cmd_status`**

Replace the existing `fn cmd_status() -> Result<()> { ... }` (the ~50-line body) with:

```rust
fn cmd_status() -> Result<()> {
    let config = Config::load()?;
    vitalog::cli::status_cmd::execute(&config)
}
```

- [ ] **Step 4: Remove now-unused imports in main.rs**

In `src/main.rs`, audit the top of file for imports that became unused after the extraction (likely `db`, `materializer`, `state`, `time`, etc., if no other helpers in `main.rs` use them). Remove any that `cargo build` flags. Leave anything still in use.

- [ ] **Step 5: Run the suite**

Run: `cargo build`
Expected: clean.

Run: `cargo test`
Expected: clean (no behaviour change; status JSON shape unchanged).

- [ ] **Step 6: Smoke-test status against the real config**

```bash
cargo run -- status | head -20
# Expected: same JSON as before the refactor, plus the materializer's
# usual on-disk effects (the .vitalog.db gets a sync pass).
```

- [ ] **Step 7: Commit**

```bash
git add src/cli/status_cmd.rs src/cli/mod.rs src/main.rs
git commit -m "$(cat <<'EOF'
refactor(status): extract cmd_status into cli/status_cmd.rs

Lifts the existing status logic into vitalog::cli::status_cmd::
execute so it can be exercised from unit tests (matching the
trend_cmd pattern). The function now opens an rw connection and
runs materializer::sync_all before reading, mirroring today_cmd
— which ensures just-logged data shows up in `vitalog status`
the same way it shows up in `vitalog today` (issue #27).

Sync errors are swallowed, same as today_cmd.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Wire reminders into `status_cmd`

**Files:**
- Modify: `src/cli/status_cmd.rs`

Goal: add the `reminders` + `reminder_warnings` arrays to the `vitalog status` JSON, matching the shape in `vitalog today --json`.

- [ ] **Step 1: Add failing tests**

Create a new test module at the bottom of `src/cli/status_cmd.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::NaiveDate;

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
        let _ = crate::materializer::sync_all(
            &conn,
            &config.notes_dir_path(),
            config,
            &registry,
        );
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
}
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib status_cmd::tests`
Expected: compile failure (`assemble_status` does not exist).

- [ ] **Step 3: Refactor `execute` to extract `assemble_status`**

In `src/cli/status_cmd.rs`, restructure so the JSON-assembly logic lives in a testable helper. Replace the body of `execute` with:

```rust
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

pub(crate) fn assemble_status(
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
            &reminders_defs,
            config,
        )?
    };
    let rs_json: Vec<serde_json::Value> = eval
        .reminders
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "display": r.display,
                "interval_days": r.interval_days,
                "last_done": r.last_done.map(|d| d.format("%Y-%m-%d").to_string()),
                "days_since": r.days_since,
                "due": r.due,
            })
        })
        .collect();
    output["reminders"] = serde_json::Value::Array(rs_json);

    let warns_json: Vec<serde_json::Value> = eval
        .warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    output["reminder_warnings"] = serde_json::Value::Array(warns_json);

    Ok(output)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib status_cmd::tests`
Expected: all three pass.

Run: `cargo test`
Expected: clean across the workspace.

- [ ] **Step 5: Commit**

```bash
git add src/cli/status_cmd.rs
git commit -m "$(cat <<'EOF'
feat(reminders): include reminders + reminder_warnings in status JSON

Extracts assemble_status into a testable helper and appends the
reminders array (always present, every reminder included) and
the reminder_warnings array (soft warnings — unknown metric
targets) to the JSON payload. Shape matches today --json.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Default config preset + README + CLAUDE.md updates

**Files:**
- Modify: `presets/default.toml`
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Append commented `[reminders]` example to the preset**

Open `presets/default.toml`. Add a new section at the end (after `# [climbing]` or wherever the file currently ends):

```toml

# [reminders]
# Smart reminders that fire when something you've configured hasn't been
# logged recently. Shown at the top of `vitalog today` and in the JSON of
# both `today` and `status`.
#
# Each reminder picks one of four "watch" sources:
#   - "metric"    — a custom metric in [metrics] above
#   - "session"   — a row in the sessions table (any training session)
#   - "lift"      — a row in lift_sets (a specific exercise)
#   - "day_field" — a built-in days column: weight, sleep_hours, mood,
#                   energy, sleep_start, sleep_end
#
# `interval_days = N` means "fire if no matching row in the last N
# calendar days" — so 2 = every other day, 1 = daily, 7 = weekly.
#
# [reminders.lactic_acid]
# display       = "Lactic acid training"
# interval_days = 2
# watch         = "metric"
# target        = "la_min"            # log via: vitalog log metric la_min 15
#
# [reminders.zone2]
# display       = "Zone 2 cardio"
# interval_days = 3
# watch         = "session"
# target        = { field = "zone2_min", min_value = 1 }
#
# [reminders.vo2_block]
# display       = "VO2max intervals"
# interval_days = 7
# watch         = "session"
# target        = { field = "type", equals = "vo2_max" }
#
# [reminders.deadlifts]
# display       = "Heavy deadlifts"
# interval_days = 7
# watch         = "lift"
# target        = { exercise = "deadlift", min_weight = 200 }
#
# [reminders.weigh_in]
# display       = "Daily weigh-in"
# interval_days = 1
# watch         = "day_field"
# target        = "weight"
```

- [ ] **Step 2: Add a README section**

Open `README.md`. After the existing `### Tier 3: Build a module (code required)` block (around line 71-73) and before the next top-level section, insert:

```markdown
## Reminders

Habits with rhythms ("do X every other day") don't fit phone alarms — skip a day and the alarms drift out of phase. Vitalog can watch the data you already log and remind you at the top of `vitalog today` when something hasn't been done recently.

```toml
[reminders.lactic_acid]
display       = "Lactic acid training"
interval_days = 2                                       # every other day
watch         = "metric"
target        = "la_min"

[reminders.zone2]
display       = "Zone 2 cardio"
interval_days = 3
watch         = "session"
target        = { field = "zone2_min", min_value = 1 }

[reminders.deadlifts]
display       = "Heavy deadlifts"
interval_days = 7
watch         = "lift"
target        = { exercise = "deadlift", min_weight = 200 }

[reminders.weigh_in]
display       = "Daily weigh-in"
interval_days = 1
watch         = "day_field"
target        = "weight"
```

Each reminder picks one of four `watch` kinds:

- **`metric`** — a custom metric from `[metrics]`. By default `value > 0` counts as "logged"; set `count_zero_as_logged = true` if 0 is a real reading you want to count.
- **`session`** — a row in the training-sessions table. Text columns (`type`, `block`, `vo2_intervals`) use `equals = "..."`; numeric columns (`duration`, `rpe`, `zone2_min`, `hr_avg`, `week`) use `min_value = N`.
- **`lift`** — a row in `lift_sets`. Requires `exercise`; optional `min_weight` (lbs) and `min_reps` narrow the match.
- **`day_field`** — one of `weight`, `sleep_hours`, `mood`, `energy`, `sleep_start`, `sleep_end`. Any non-null value counts as "logged".

A reminder fires when the most recent matching date is either absent or at least `interval_days` calendar days before today (respecting `day_start_hour`). The block is silent when nothing is due. Both `vitalog today --json` and `vitalog status` always include a `reminders` array (every configured reminder, due or not) plus a `reminder_warnings` sibling — handy for piping into a notification script.
```

(Markdown nesting: the triple-backtick fences are part of the README content; preserve them when copying into the file.)

- [ ] **Step 3: Update CLAUDE.md file map**

In `vitalog/CLAUDE.md`, find the `## File Map` section and:

(a) Add `reminders.rs` to the top-level `src/` listing, alphabetically. The current top-level entries are around `food_sum.rs`, `frontmatter.rs`, etc. (Read the current map; insert in the right spot, with the description: `Reminder definitions, evaluator, soft warnings; reads existing tables only.`)

(b) Add `cli/status_cmd.rs` to the `src/cli/` listing with: `vitalog status — JSON output, sync-on-run, reminders array.`

(c) Amend the `today_cmd.rs` entry to mention reminders: append `; loads & renders [reminders] block above the summary` to the existing description (or insert a similar phrase that fits the existing tone).

- [ ] **Step 4: Run the tests one more time**

Run: `cargo test`
Expected: clean.

Run: `cargo run -- readme | head -40`
Expected: the README is embedded — the new "Reminders" section should appear in the output near the top of where the file places it.

- [ ] **Step 5: Commit**

```bash
git add presets/default.toml README.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs(reminders): document [reminders] in README, preset, CLAUDE.md

Adds a "Reminders" section to README with the TOML schema and
behaviour, a commented-out [reminders] example to the default
preset, and entries in CLAUDE.md's file map for the new
reminders.rs and status_cmd.rs files.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: End-to-end integration test

**Files:**
- Create: `tests/reminders.rs`

Goal: assert the full pipeline — config + notes + DB sync + `today --text` and `status --json` — over a tempdir-backed config, mirroring the existing `tests/today.rs` and `tests/trend.rs` patterns.

- [ ] **Step 1: Create the integration test file**

Create `tests/reminders.rs`:

```rust
//! End-to-end tests for the [reminders] feature.

use chrono::NaiveDate;
use vitalog::cli::status_cmd;
use vitalog::cli::today_cmd::{
    assemble, render_json_with_reminders, render_reminders_block, render_text,
};
use vitalog::config::Config;
use vitalog::db;
use vitalog::goals::load_goals;
use vitalog::modules;
use vitalog::reminders::{evaluate, load_reminders};

fn setup_with_reminders(reminders_toml: &str) -> (tempfile::TempDir, Config) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().display().to_string().replace('\\', "/");
    let toml_str = format!(
        r#"
notes_dir = "{path}"
time_format = "24h"
weight_unit = "kg"

[metrics]
la_min = {{ display = "Lactic acid (min)", color = "red", unit = "min" }}

[exercises]
deadlift = {{ display = "Deadlift", color = "yellow" }}

{reminders_toml}
"#
    );
    let config: Config = toml::from_str(&toml_str).unwrap();
    (dir, config)
}

fn write_note(notes_dir: &std::path::Path, date: &str, body: &str) {
    std::fs::write(notes_dir.join(format!("{date}.md")), body).unwrap();
}

#[test]
fn today_text_shows_overdue_lactic_acid_reminder() {
    let (dir, config) = setup_with_reminders(
        r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
    );

    // Last LA session was 3 days before our test "today".
    write_note(
        dir.path(),
        "2026-05-09",
        "---\ndate: 2026-05-09\nla_min: 15\n---\n\n## Food\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();
    let reminders = load_reminders(&config).unwrap();
    let eval = evaluate(&conn, date, &reminders, &config).unwrap();

    let block = render_reminders_block(&eval.reminders, false);
    let body = render_text(&summary, &goals, false);

    assert!(block.contains("Reminders"), "got:\n{block}");
    assert!(block.contains("Lactic acid training"), "got:\n{block}");
    assert!(block.contains("3 days ago"), "got:\n{block}");
    assert!(block.contains("2026-05-09"), "got:\n{block}");

    // Combined: reminder block precedes date header.
    let combined = format!("{block}{body}");
    let header_idx = combined.find("2026-05-12 — Daily summary").unwrap();
    let rem_idx = combined.find("Lactic acid training").unwrap();
    assert!(rem_idx < header_idx, "got:\n{combined}");
}

#[test]
fn today_text_silent_when_no_reminder_due() {
    let (dir, config) = setup_with_reminders(
        r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
    );

    // Logged today → not due.
    write_note(
        dir.path(),
        "2026-05-12",
        "---\ndate: 2026-05-12\nla_min: 15\n---\n\n## Food\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
    let reminders = load_reminders(&config).unwrap();
    let eval = evaluate(&conn, date, &reminders, &config).unwrap();

    let block = render_reminders_block(&eval.reminders, false);
    assert!(block.is_empty(), "expected empty block, got:\n{block:?}");
}

#[test]
fn today_json_includes_all_reminders_including_not_due() {
    let (dir, config) = setup_with_reminders(
        r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"

[reminders.weigh_in]
display = "Daily weigh-in"
interval_days = 1
watch = "day_field"
target = "weight"
"#,
    );

    // LA done today (not due), weight never logged (due).
    write_note(
        dir.path(),
        "2026-05-12",
        "---\ndate: 2026-05-12\nla_min: 15\n---\n\n## Food\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();
    let reminders = load_reminders(&config).unwrap();
    let eval = evaluate(&conn, date, &reminders, &config).unwrap();

    let v = render_json_with_reminders(&summary, &goals, &eval.reminders, &eval.warnings);
    let arr = v["reminders"].as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let la = arr.iter().find(|r| r["id"] == "lactic_acid").unwrap();
    assert_eq!(la["due"], false);
    assert_eq!(la["last_done"], "2026-05-12");
    assert_eq!(la["days_since"], 0);

    let weigh = arr.iter().find(|r| r["id"] == "weigh_in").unwrap();
    assert_eq!(weigh["due"], true);
    assert!(weigh["last_done"].is_null());
}

#[test]
fn status_json_includes_reminders_after_sync() {
    let (dir, config) = setup_with_reminders(
        r#"
[reminders.lactic_acid]
display = "Lactic acid training"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
    );

    write_note(
        dir.path(),
        "2026-05-09",
        "---\ndate: 2026-05-09\nla_min: 15\n---\n\n## Food\n",
    );

    // Pre-create the DB so status_cmd::execute can read it. We don't run
    // sync here — status_cmd will do it internally.
    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    drop(conn);

    // Use the testable helper rather than capturing stdout from execute.
    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    let _ = vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry);
    let v = status_cmd::assemble_status(&conn, &config, &registry).unwrap();

    let arr = v["reminders"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "lactic_acid");
    // The "due" state will depend on today's date when the test runs.
    // We assert the shape, not the value.
    assert!(arr[0]["last_done"].is_string() || arr[0]["last_done"].is_null());
}
```

- [ ] **Step 2: Make `assemble_status` reachable from integration tests**

In `src/cli/status_cmd.rs`, change `pub(crate) fn assemble_status` to `pub fn assemble_status` so the integration test can call it. The function isn't part of the user-facing CLI surface, but it's the only stable hook into the JSON for tests.

- [ ] **Step 3: Run the integration suite**

Run: `cargo test --test reminders`
Expected: all four tests pass.

Run: `cargo test`
Expected: full suite clean.

- [ ] **Step 4: Run lint + fmt**

Run: `cargo fmt --check`
Expected: clean.

Run: `cargo clippy -- -D warnings`
Expected: clean.

- [ ] **Step 5: Smoke-test against the real config one more time**

Use a far-future date per `vitalog-workspace/CLAUDE.md`:

```bash
cargo run -- today 2099-01-01 | head -20
cargo run -- status | jq '.reminders, .reminder_warnings' | head -40
```

Expected: `today` prints the reminder block (or stays silent if all your reminders are recent against your real data). `status` prints the `reminders` and `reminder_warnings` arrays.

If you wrote any test notes to `~/vitalog-notes/2099-01-01.md` during smoke-testing, delete them now:

```bash
rm -f ~/vitalog-notes/2099-01-01.md ~/vitalog-notes/1999-01-01.md
```

- [ ] **Step 6: Commit**

```bash
git add tests/reminders.rs src/cli/status_cmd.rs
git commit -m "$(cat <<'EOF'
test(reminders): end-to-end integration over tempdir config

Exercises the full pipeline: config + notes + DB sync + render
for both `today` (text and JSON) and `status` (JSON). Asserts
the reminder block precedes the summary date header, silent-day
case emits no block, JSON lists all reminders (due and not),
and the status JSON matches the today --json shape.

Promotes assemble_status to pub so integration tests can reach
it without scraping stdout.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review Checklist (run after completing all tasks)

- [ ] Run `cargo test` end-to-end on the final commit.
- [ ] Run `cargo fmt --check` and `cargo clippy -- -D warnings`.
- [ ] `git log --oneline` reads as a coherent sequence (Task 1–12 each a single commit).
- [ ] `cargo run -- today --help` still works.
- [ ] `cargo run -- status` prints valid JSON (pipe to `jq .` if uncertain).
- [ ] `~/.config/vitalog/config.toml` with no `[reminders]` block still works (the field is `#[serde(default)]`).
- [ ] If you added test notes during smoke testing, they're deleted.
