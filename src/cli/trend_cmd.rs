//! `vitalog trend <field> [days]` — print a chart of recent values.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
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

#[derive(Debug, Clone)]
pub struct TrendData {
    pub field: TrendField,
    pub days: u32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub points: Vec<(NaiveDate, Option<f64>)>,
    pub stats: TrendStats,
}

const BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const CHART_ROWS: usize = 8;
const COL_WIDTH: usize = 2;
const Y_LABEL_WIDTH: usize = 5;

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

/// Query the relevant table for `field` over `[from, to]` (inclusive),
/// then expand to a Vec spanning every day in the window — gap days
/// carry `None`. Rows with NULL values are filtered at the SQL layer
/// (treated as gaps).
pub fn build_window(
    field: &TrendField,
    conn: &Connection,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<(NaiveDate, Option<f64>)>> {
    let from_str = from.format("%Y-%m-%d").to_string();
    let to_str = to.format("%Y-%m-%d").to_string();
    let rows: Vec<(String, f64)> = match &field.source {
        TrendSource::DaysColumn(col) => {
            // Safe: `col` is a &'static str from the BUILTINS allowlist.
            let sql = format!(
                "SELECT date, {col} FROM days \
                 WHERE date BETWEEN ?1 AND ?2 AND {col} IS NOT NULL \
                 ORDER BY date ASC"
            );
            let mut stmt = conn.prepare(&sql)?;
            let result = stmt
                .query_map([&from_str, &to_str], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            result
        }
        TrendSource::Metric(name) => {
            let mut stmt = conn.prepare(
                "SELECT date, value FROM metrics \
                 WHERE name = ?1 AND date BETWEEN ?2 AND ?3 \
                 ORDER BY date ASC",
            )?;
            let result = stmt
                .query_map(
                    rusqlite::params![name, &from_str, &to_str],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            result
        }
    };
    let map: HashMap<String, f64> = rows.into_iter().collect();
    let total_days = (to - from).num_days() as usize + 1;
    let mut out = Vec::with_capacity(total_days);
    let mut day = from;
    while day <= to {
        let key = day.format("%Y-%m-%d").to_string();
        out.push((day, map.get(&key).copied()));
        day = day.succ_opt().expect("date overflow inside trend window");
    }
    Ok(out)
}

/// Pure(ish) data assembly: resolves the field, queries the DB, computes
/// stats. The caller is responsible for sync-on-run; this fn just reads.
/// `today` is parameterized for testability — production callers pass
/// `config.effective_today_date()`.
pub fn assemble(
    field: &str,
    days: u32,
    config: &Config,
    conn: &Connection,
    today: NaiveDate,
) -> Result<TrendData> {
    let trend_field = resolve_field(field, config, conn)?;
    let to = today;
    let from = to - Duration::days(days as i64 - 1);
    let points = build_window(&trend_field, conn, from, to)?;
    let stats = compute_stats(&points);
    Ok(TrendData {
        field: trend_field,
        days,
        from,
        to,
        points,
        stats,
    })
}

fn unit_clause(unit: &Option<String>) -> String {
    match unit {
        Some(u) => format!(", {u}"),
        None => String::new(),
    }
}

fn slope_units(unit: &Option<String>) -> (String, String) {
    match unit {
        Some(u) => (format!("{u}/day"), format!("{u}/week")),
        None => ("per day".to_string(), "per week".to_string()),
    }
}

pub fn render_compact(data: &TrendData) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} ({}d{}): ",
        data.field.display,
        data.days,
        unit_clause(&data.field.unit)
    ));

    match (data.stats.min, data.stats.max) {
        (Some(min), Some(max)) => {
            let span = max - min;
            for (_, v) in &data.points {
                match v {
                    None => out.push(' '),
                    Some(x) => {
                        let idx = if span == 0.0 {
                            3
                        } else {
                            (((x - min) / span) * 7.0).round() as usize
                        };
                        out.push(BLOCKS[idx.min(7)]);
                    }
                }
            }
            out.push('\n');
            out.push_str(&format!(
                "mean {:.1}  min {:.1}  max {:.1}\n",
                data.stats.mean.unwrap(),
                min,
                max
            ));
            if let (Some(d), Some(w)) = (data.stats.slope_per_day, data.stats.slope_per_week) {
                let (per_day, per_week) = slope_units(&data.field.unit);
                out.push_str(&format!(
                    "slope {:+.2} {}  (≈ {:+.1} {})\n",
                    d, per_day, w, per_week
                ));
            }
        }
        _ => {
            for _ in &data.points {
                out.push(' ');
            }
            out.push('\n');
            out.push_str(&format!("count {}\n", data.stats.count));
        }
    }

    out
}

pub fn render_chart(data: &TrendData) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} (last {} days{})\n\n",
        data.field.display,
        data.days,
        unit_clause(&data.field.unit)
    ));

    if data.stats.count == 0 {
        out.push_str(&format!(
            "no data for {} in the last {} days\n",
            data.field.name, data.days
        ));
        return out;
    }

    let min = data.stats.min.unwrap();
    let max = data.stats.max.unwrap();
    let span = max - min;

    let row_value = |row: usize| -> f64 {
        if span == 0.0 {
            min
        } else {
            max - (row as f64 / (CHART_ROWS - 1) as f64) * span
        }
    };
    let row_for = |v: f64| -> usize {
        if span == 0.0 {
            CHART_ROWS / 2
        } else {
            let r = ((max - v) / span * (CHART_ROWS - 1) as f64).round() as usize;
            r.min(CHART_ROWS - 1)
        }
    };

    for r in 0..CHART_ROWS {
        let label = if data.field.integer_valued {
            format!("{:>width$}", row_value(r) as i64, width = Y_LABEL_WIDTH)
        } else {
            format!("{:>width$.1}", row_value(r), width = Y_LABEL_WIDTH)
        };
        out.push_str(&label);
        out.push_str(" ┤");
        for (_, v) in &data.points {
            match v {
                Some(x) if row_for(*x) == r => {
                    out.push('●');
                    for _ in 1..COL_WIDTH {
                        out.push(' ');
                    }
                }
                _ => {
                    for _ in 0..COL_WIDTH {
                        out.push(' ');
                    }
                }
            }
        }
        out.push('\n');
    }

    // Axis line
    out.push_str(&format!("{} └", " ".repeat(Y_LABEL_WIDTH)));
    for _ in 0..(data.points.len() * COL_WIDTH) {
        out.push('─');
    }
    out.push('\n');

    // Date labels (left-aligned MM-DD on the from side, right-aligned on the to side).
    let from_str = data.from.format("%m-%d").to_string();
    let to_str = data.to.format("%m-%d").to_string();
    let total_width = data.points.len() * COL_WIDTH;
    let pad = total_width.saturating_sub(from_str.len() + to_str.len());
    out.push_str(&format!(
        "{} {}{}{}\n",
        " ".repeat(Y_LABEL_WIDTH),
        from_str,
        " ".repeat(pad),
        to_str
    ));

    out.push('\n');
    out.push_str(&format!(
        "mean: {:.1}  min: {:.1}  max: {:.1}\n",
        data.stats.mean.unwrap(),
        min,
        max
    ));
    if let (Some(d), Some(w)) = (data.stats.slope_per_day, data.stats.slope_per_week) {
        let (per_day, per_week) = slope_units(&data.field.unit);
        out.push_str(&format!(
            "linear trend: {:+.2} {}  (≈ {:+.1} {})\n",
            d, per_day, w, per_week
        ));
    }

    out
}

pub fn render_json(data: &TrendData) -> serde_json::Value {
    let points: Vec<serde_json::Value> = data
        .points
        .iter()
        .map(|(date, value)| {
            serde_json::json!({
                "date": date.format("%Y-%m-%d").to_string(),
                "value": value,
            })
        })
        .collect();
    serde_json::json!({
        "field": data.field.name,
        "display": data.field.display,
        "unit": data.field.unit,
        "days": data.days,
        "from": data.from.format("%Y-%m-%d").to_string(),
        "to": data.to.format("%Y-%m-%d").to_string(),
        "points": points,
        "stats": {
            "count": data.stats.count,
            "mean": data.stats.mean,
            "min": data.stats.min,
            "max": data.stats.max,
            "slope_per_day": data.stats.slope_per_day,
            "slope_per_week": data.stats.slope_per_week,
        },
    })
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

    fn seed_day(conn: &rusqlite::Connection, date: &str, weight: Option<f64>) {
        conn.execute(
            "INSERT INTO days (date, weight, file_mtime) VALUES (?1, ?2, 0.0)",
            rusqlite::params![date, weight],
        )
        .unwrap();
    }

    fn weight_field(unit: &str) -> TrendField {
        TrendField {
            name: "weight".to_string(),
            source: TrendSource::DaysColumn("weight"),
            display: "weight".to_string(),
            unit: Some(unit.to_string()),
            integer_valued: false,
        }
    }

    fn metric_field(name: &str) -> TrendField {
        TrendField {
            name: name.to_string(),
            source: TrendSource::Metric(name.to_string()),
            display: name.to_string(),
            unit: None,
            integer_valued: false,
        }
    }

    #[test]
    fn build_window_days_field_fills_gaps_with_none() {
        let conn = empty_db();
        seed_day(&conn, "2026-01-01", Some(120.0));
        seed_day(&conn, "2026-01-02", None); // present but no weight
        seed_day(&conn, "2026-01-04", Some(121.5));
        // 2026-01-03 not seeded at all

        let from = d(2026, 1, 1);
        let to = d(2026, 1, 4);
        let pts = build_window(&weight_field("kg"), &conn, from, to).unwrap();
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], (d(2026, 1, 1), Some(120.0)));
        assert_eq!(pts[1], (d(2026, 1, 2), None));
        assert_eq!(pts[2], (d(2026, 1, 3), None));
        assert_eq!(pts[3], (d(2026, 1, 4), Some(121.5)));
    }

    #[test]
    fn build_window_metric_field_filters_by_name() {
        let conn = empty_db();
        seed_day(&conn, "2026-01-01", None);
        seed_day(&conn, "2026-01-02", None);
        conn.execute(
            "INSERT INTO metrics (date, name, value) VALUES ('2026-01-01', 'rh', 60.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metrics (date, name, value) VALUES ('2026-01-02', 'other', 99.0)",
            [],
        )
        .unwrap();

        let pts = build_window(&metric_field("rh"), &conn, d(2026, 1, 1), d(2026, 1, 2)).unwrap();
        assert_eq!(pts, vec![
            (d(2026, 1, 1), Some(60.0)),
            (d(2026, 1, 2), None),
        ]);
    }

    #[test]
    fn build_window_empty_returns_all_none() {
        let conn = empty_db();
        let pts = build_window(&weight_field("kg"), &conn, d(2026, 1, 1), d(2026, 1, 3)).unwrap();
        assert_eq!(pts.len(), 3);
        assert!(pts.iter().all(|(_, v)| v.is_none()));
    }

    #[test]
    fn assemble_smoke() {
        let toml_str = "notes_dir = \"/tmp\"\nweight_unit = \"kg\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        let conn = empty_db();
        seed_day(&conn, "2026-01-01", Some(120.0));
        seed_day(&conn, "2026-01-02", Some(120.5));

        let to = d(2026, 1, 3);
        let data = assemble("weight", 3, &config, &conn, to).unwrap();

        assert_eq!(data.days, 3);
        assert_eq!(data.from, d(2026, 1, 1));
        assert_eq!(data.to, d(2026, 1, 3));
        assert_eq!(data.points.len(), 3);
        assert_eq!(data.points[2].1, None); // 2026-01-03 has no row
        assert_eq!(data.stats.count, 2);
        assert_eq!(data.field.unit.as_deref(), Some("kg"));
    }

    fn make_data(field: TrendField, days: u32, points: Vec<(NaiveDate, Option<f64>)>) -> TrendData {
        let from = points.first().map(|(d, _)| *d).unwrap_or(d(2026, 1, 1));
        let to = points.last().map(|(d, _)| *d).unwrap_or(from);
        let stats = compute_stats(&points);
        TrendData { field, days, from, to, points, stats }
    }

    #[test]
    fn compact_renders_blocks_and_stats() {
        let pts: Vec<_> = (0..7)
            .map(|i| (d(2026, 1, (i + 1) as u32), Some(120.0 + i as f64 * 0.5)))
            .collect();
        let data = make_data(weight_field("kg"), 7, pts);
        let s = render_compact(&data);
        assert!(s.starts_with("weight (7d, kg): "), "got: {s}");
        // 7 points, monotonic increase → blocks span low to high
        assert!(s.contains('▁'), "got: {s}");
        assert!(s.contains('█'), "got: {s}");
        assert!(s.contains("mean 121.5"), "got: {s}");
        assert!(s.contains("min 120.0"), "got: {s}");
        assert!(s.contains("max 123.0"), "got: {s}");
        assert!(s.contains("slope +0.50 kg/day"), "got: {s}");
        assert!(s.contains("≈ +3.5 kg/week"), "got: {s}");
    }

    #[test]
    fn compact_all_equal_uses_mid_block() {
        let pts: Vec<_> = (0..3)
            .map(|i| (d(2026, 1, (i + 1) as u32), Some(120.0)))
            .collect();
        let data = make_data(weight_field("kg"), 3, pts);
        let s = render_compact(&data);
        assert!(s.contains("▄▄▄"), "expected three mid blocks, got: {s}");
    }

    #[test]
    fn compact_gap_renders_space() {
        let pts = vec![
            (d(2026, 1, 1), Some(120.0)),
            (d(2026, 1, 2), None),
            (d(2026, 1, 3), Some(121.0)),
        ];
        let data = make_data(weight_field("kg"), 3, pts);
        let s = render_compact(&data);
        // Expect ' ' between two blocks
        assert!(s.contains("▁ █") || s.contains("▄ ▄"), "got: {s}");
    }

    #[test]
    fn compact_no_data_omits_slope_line() {
        let pts = vec![(d(2026, 1, 1), None), (d(2026, 1, 2), None)];
        let data = make_data(weight_field("kg"), 2, pts);
        let s = render_compact(&data);
        assert!(!s.contains("slope"), "got: {s}");
        assert!(s.contains("count 0"), "got: {s}");
    }

    #[test]
    fn chart_no_data_short_circuits() {
        let pts: Vec<(NaiveDate, Option<f64>)> = (0..3)
            .map(|i| (d(2026, 1, (i + 1) as u32), None))
            .collect();
        let data = make_data(weight_field("kg"), 3, pts);
        let s = render_chart(&data);
        assert!(s.contains("weight (last 3 days, kg)"), "got: {s}");
        assert!(s.contains("no data for weight in the last 3 days"), "got: {s}");
        assert!(!s.contains("┤"), "should skip axis when empty: {s}");
    }

    #[test]
    fn chart_renders_axis_and_dots_for_known_series() {
        // Known fixture: 5 days, monotonic.
        let pts = vec![
            (d(2026, 1, 1), Some(120.0)),
            (d(2026, 1, 2), Some(120.5)),
            (d(2026, 1, 3), Some(121.0)),
            (d(2026, 1, 4), Some(121.5)),
            (d(2026, 1, 5), Some(122.0)),
        ];
        let data = make_data(weight_field("kg"), 5, pts);
        let s = render_chart(&data);

        // Title
        assert!(s.contains("weight (last 5 days, kg)"), "got:\n{s}");
        // Axis exists
        assert!(s.contains("┤"), "got:\n{s}");
        assert!(s.contains("└"), "got:\n{s}");
        // 8 data rows means 8 lines containing '┤'
        let row_count = s.matches('┤').count();
        assert_eq!(row_count, 8, "got:\n{s}");
        // At least one '●'
        assert!(s.contains('●'), "got:\n{s}");
        // Date labels MM-DD
        assert!(s.contains("01-01"), "got:\n{s}");
        assert!(s.contains("01-05"), "got:\n{s}");
        // Stats
        assert!(s.contains("mean: 121.0"), "got:\n{s}");
        assert!(s.contains("min: 120.0"), "got:\n{s}");
        assert!(s.contains("max: 122.0"), "got:\n{s}");
        assert!(s.contains("linear trend: +0.50 kg/day"), "got:\n{s}");
    }

    #[test]
    fn chart_integer_field_renders_integer_y_labels() {
        let pts = vec![
            (d(2026, 1, 1), Some(3.0)),
            (d(2026, 1, 2), Some(5.0)),
            (d(2026, 1, 3), Some(7.0)),
        ];
        let mood = TrendField {
            name: "mood".to_string(),
            source: TrendSource::DaysColumn("mood"),
            display: "mood".to_string(),
            unit: None,
            integer_valued: true,
        };
        let data = make_data(mood, 3, pts);
        let s = render_chart(&data);
        // Y-axis labels should be integers — no decimal in the label band.
        // The "axis label band" is the first 5 chars of every line containing '┤'.
        for line in s.lines().filter(|l| l.contains('┤')) {
            // chars before '┤'
            let label = line.split('┤').next().unwrap();
            assert!(!label.contains('.'), "y-label should be integer: '{label}' in:\n{s}");
        }
        // No unit in title
        assert!(s.contains("mood (last 3 days)"), "got:\n{s}");
        assert!(!s.contains("(last 3 days,"), "no unit clause: {s}");
    }

    #[test]
    fn json_includes_full_window_with_nulls() {
        let pts = vec![
            (d(2026, 1, 1), Some(120.0)),
            (d(2026, 1, 2), None),
            (d(2026, 1, 3), Some(121.0)),
        ];
        let data = make_data(weight_field("kg"), 3, pts);
        let v = render_json(&data);
        assert_eq!(v["field"], "weight");
        assert_eq!(v["display"], "weight");
        assert_eq!(v["unit"], "kg");
        assert_eq!(v["days"], 3);
        assert_eq!(v["from"], "2026-01-01");
        assert_eq!(v["to"], "2026-01-03");
        let pts_json = v["points"].as_array().unwrap();
        assert_eq!(pts_json.len(), 3);
        assert_eq!(pts_json[0]["date"], "2026-01-01");
        assert_eq!(pts_json[0]["value"], 120.0);
        assert!(pts_json[1]["value"].is_null());
        assert_eq!(pts_json[2]["value"], 121.0);
        assert_eq!(v["stats"]["count"], 2);
        assert_eq!(v["stats"]["min"], 120.0);
        assert_eq!(v["stats"]["max"], 121.0);
        assert!(v["stats"]["slope_per_day"].is_f64());
    }

    #[test]
    fn json_empty_window_has_null_stats() {
        let pts: Vec<(NaiveDate, Option<f64>)> = (0..3)
            .map(|i| (d(2026, 1, (i + 1) as u32), None))
            .collect();
        let data = make_data(weight_field("kg"), 3, pts);
        let v = render_json(&data);
        assert_eq!(v["stats"]["count"], 0);
        assert!(v["stats"]["mean"].is_null());
        assert!(v["stats"]["slope_per_day"].is_null());
        assert!(v["stats"]["slope_per_week"].is_null());
    }

    #[test]
    fn json_unit_null_for_no_unit_field() {
        let mood = TrendField {
            name: "mood".to_string(),
            source: TrendSource::DaysColumn("mood"),
            display: "mood".to_string(),
            unit: None,
            integer_valued: true,
        };
        let pts = vec![(d(2026, 1, 1), Some(5.0))];
        let data = make_data(mood, 1, pts);
        let v = render_json(&data);
        assert!(v["unit"].is_null());
    }
}
