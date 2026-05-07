//! `vitalog trend <field> [days]` — print a chart of recent values.

use chrono::NaiveDate;
use color_eyre::eyre::{eyre, Result};
use rusqlite::{Connection, OptionalExtension};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum TrendSource {
    /// Column on the `days` table. The column name is from a hardcoded
    /// allowlist (see `BUILTINS`) and is safe to interpolate into SQL.
    DaysColumn(&'static str),
    /// Row in the `metrics` table where `name = ?`.
    Metric(String),
}

#[derive(Debug, Clone)]
pub struct TrendField {
    /// User-provided name; appears in JSON output as `field`.
    pub name: String,
    pub source: TrendSource,
    /// Display label; same as `name` for built-ins, from config for metrics.
    pub display: String,
    pub unit: Option<String>,
    /// Render y-axis labels as integers (true for `mood`, `energy`).
    pub integer_valued: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrendStats {
    pub count: usize,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Ordinary least squares slope on (day_index, value). None when count < 2.
    pub slope_per_day: Option<f64>,
    /// `slope_per_day * 7`.
    pub slope_per_week: Option<f64>,
}

/// Built-in fields served by the `days` table.
/// (name, column, integer_valued)
const BUILTINS: &[(&str, &str, bool)] = &[
    ("weight", "weight", false),
    ("sleep_hours", "sleep_hours", false),
    ("mood", "mood", true),
    ("energy", "energy", true),
];

/// Resolve a user-supplied field name into a `TrendField`. Tries built-ins
/// first, then `config.metrics`, then a soft-resolve against historical
/// rows in the `metrics` table (so a previously-configured-now-removed
/// metric still works).
pub fn resolve_field(name: &str, config: &Config, conn: &Connection) -> Result<TrendField> {
    for (bname, col, int_valued) in BUILTINS {
        if name == *bname {
            let unit = match *bname {
                "weight" => Some(config.weight_unit.to_string()),
                "sleep_hours" => Some("h".to_string()),
                _ => None,
            };
            return Ok(TrendField {
                name: name.to_string(),
                source: TrendSource::DaysColumn(col),
                display: name.to_string(),
                unit,
                integer_valued: *int_valued,
            });
        }
    }
    if let Some(m) = config.metrics.get(name) {
        return Ok(TrendField {
            name: name.to_string(),
            source: TrendSource::Metric(name.to_string()),
            display: m.display.clone(),
            unit: m.unit.clone(),
            integer_valued: false,
        });
    }
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM metrics WHERE name = ?1 LIMIT 1",
            [name],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if exists {
        return Ok(TrendField {
            name: name.to_string(),
            source: TrendSource::Metric(name.to_string()),
            display: name.to_string(),
            unit: None,
            integer_valued: false,
        });
    }
    let mut known: Vec<String> = BUILTINS.iter().map(|(n, _, _)| n.to_string()).collect();
    let mut configured: Vec<String> = config.metrics.keys().cloned().collect();
    configured.sort();
    known.extend(configured);
    Err(eyre!(
        "unknown field '{name}'. Known fields: {}",
        known.join(", ")
    ))
}

pub fn execute(_field: &str, _days: u32, _compact: bool, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("trend command not yet implemented");
}

/// Mean / min / max / OLS slope over the values in `points`. Days with
/// `None` are skipped for stats (but their indices still count toward
/// the slope's x-axis, so a gap in the middle pulls the slope correctly).
pub fn compute_stats(points: &[(NaiveDate, Option<f64>)]) -> TrendStats {
    let xs_ys: Vec<(usize, f64)> = points
        .iter()
        .enumerate()
        .filter_map(|(i, (_, v))| v.map(|x| (i, x)))
        .collect();
    let count = xs_ys.len();
    if count == 0 {
        return TrendStats {
            count: 0,
            mean: None,
            min: None,
            max: None,
            slope_per_day: None,
            slope_per_week: None,
        };
    }
    let mean = xs_ys.iter().map(|(_, y)| *y).sum::<f64>() / count as f64;
    let min = xs_ys.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let max = xs_ys.iter().map(|(_, y)| *y).fold(f64::NEG_INFINITY, f64::max);
    let (slope_per_day, slope_per_week) = if count < 2 {
        (None, None)
    } else {
        let n = count as f64;
        let x_mean = xs_ys.iter().map(|(x, _)| *x as f64).sum::<f64>() / n;
        let num: f64 = xs_ys
            .iter()
            .map(|(x, y)| (*x as f64 - x_mean) * (y - mean))
            .sum();
        let den: f64 = xs_ys
            .iter()
            .map(|(x, _)| (*x as f64 - x_mean).powi(2))
            .sum();
        // xs come from enumerate(), so with count >= 2 there are always at least
        // two distinct indices (0, 1, …) — den cannot be zero.
        debug_assert!(den != 0.0, "denominator must be non-zero: enumerate() guarantees distinct x values");
        let slope = num / den;
        (Some(slope), Some(slope * 7.0))
    };
    TrendStats {
        count,
        mean: Some(mean),
        min: Some(min),
        max: Some(max),
        slope_per_day,
        slope_per_week,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn config_with_metric(name: &str, display: &str, unit: Option<&str>) -> Config {
        let unit_clause = match unit {
            Some(u) => format!(", unit = \"{u}\""),
            None => String::new(),
        };
        let toml_str = format!(
            "notes_dir = \"/tmp\"\n[metrics]\n{name} = {{ display = \"{display}\", color = \"red\"{unit_clause} }}\n"
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn empty_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::CORE_SCHEMA_TEST_HOOK).unwrap();
        conn
    }

    #[test]
    fn resolve_builtin_weight_uses_config_unit() {
        let toml_str = "notes_dir = \"/tmp\"\nweight_unit = \"kg\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let conn = empty_db();
        let f = resolve_field("weight", &config, &conn).unwrap();
        assert_eq!(f.name, "weight");
        assert!(matches!(f.source, TrendSource::DaysColumn("weight")));
        assert_eq!(f.unit.as_deref(), Some("kg"));
        assert!(!f.integer_valued);
    }

    #[test]
    fn resolve_builtin_mood_is_integer_valued() {
        let toml_str = "notes_dir = \"/tmp\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let conn = empty_db();
        let f = resolve_field("mood", &config, &conn).unwrap();
        assert!(matches!(f.source, TrendSource::DaysColumn("mood")));
        assert!(f.integer_valued);
        assert!(f.unit.is_none());
    }

    #[test]
    fn resolve_configured_metric_uses_config_display_and_unit() {
        let config = config_with_metric("resting_hr", "Resting HR", Some("bpm"));
        let conn = empty_db();
        let f = resolve_field("resting_hr", &config, &conn).unwrap();
        assert!(matches!(&f.source, TrendSource::Metric(n) if n == "resting_hr"));
        assert_eq!(f.display, "Resting HR");
        assert_eq!(f.unit.as_deref(), Some("bpm"));
    }

    #[test]
    fn resolve_historical_metric_falls_back_to_raw_name() {
        let toml_str = "notes_dir = \"/tmp\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let conn = empty_db();
        // Seed a row in metrics so the soft-resolve path triggers.
        conn.execute(
            "INSERT INTO days (date, file_mtime) VALUES ('2026-01-01', 0.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metrics (date, name, value) VALUES ('2026-01-01', 'old_metric', 1.0)",
            [],
        )
        .unwrap();
        let f = resolve_field("old_metric", &config, &conn).unwrap();
        assert!(matches!(&f.source, TrendSource::Metric(n) if n == "old_metric"));
        assert_eq!(f.display, "old_metric");
        assert!(f.unit.is_none());
    }

    #[test]
    fn resolve_unknown_lists_known_fields() {
        let config = config_with_metric("resting_hr", "Resting HR", Some("bpm"));
        let conn = empty_db();
        let err = resolve_field("nonsense", &config, &conn).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("nonsense"), "got: {msg}");
        assert!(msg.contains("weight"), "got: {msg}");
        assert!(msg.contains("resting_hr"), "got: {msg}");
    }

    #[test]
    fn stats_empty() {
        let stats = compute_stats(&[]);
        assert_eq!(stats.count, 0);
        assert!(stats.mean.is_none());
        assert!(stats.slope_per_day.is_none());
    }

    #[test]
    fn stats_all_none_is_empty() {
        let pts = vec![(d(2026, 1, 1), None), (d(2026, 1, 2), None)];
        let stats = compute_stats(&pts);
        assert_eq!(stats.count, 0);
        assert!(stats.mean.is_none());
        assert!(stats.min.is_none());
        assert!(stats.max.is_none());
        assert!(stats.slope_per_day.is_none());
        assert!(stats.slope_per_week.is_none());
    }

    #[test]
    fn stats_single_point_has_no_slope() {
        let pts = vec![(d(2026, 1, 1), Some(120.0))];
        let stats = compute_stats(&pts);
        assert_eq!(stats.count, 1);
        assert_eq!(stats.mean, Some(120.0));
        assert_eq!(stats.min, Some(120.0));
        assert_eq!(stats.max, Some(120.0));
        assert!(stats.slope_per_day.is_none());
    }

    #[test]
    fn stats_linear_input_recovers_slope() {
        // y = 100 + 0.5 * day_index over 5 days, no gaps
        let pts: Vec<_> = (0..5)
            .map(|i| (d(2026, 1, (i + 1) as u32), Some(100.0 + 0.5 * i as f64)))
            .collect();
        let stats = compute_stats(&pts);
        assert_eq!(stats.count, 5);
        let slope = stats.slope_per_day.unwrap();
        assert!((slope - 0.5).abs() < 1e-9, "got {slope}");
        let weekly = stats.slope_per_week.unwrap();
        assert!((weekly - 3.5).abs() < 1e-9, "got {weekly}");
    }

    #[test]
    fn stats_gap_does_not_break_slope() {
        // Same series but the middle point is missing — slope should still be 0.5.
        let pts = vec![
            (d(2026, 1, 1), Some(100.0)),
            (d(2026, 1, 2), Some(100.5)),
            (d(2026, 1, 3), None),
            (d(2026, 1, 4), Some(101.5)),
            (d(2026, 1, 5), Some(102.0)),
        ];
        let stats = compute_stats(&pts);
        let slope = stats.slope_per_day.unwrap();
        assert!((slope - 0.5).abs() < 1e-9, "got {slope}");
    }
}
