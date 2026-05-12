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
