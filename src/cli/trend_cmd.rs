//! `vitalog trend <field> [days]` — print a chart of recent values.

use chrono::NaiveDate;
use color_eyre::eyre::Result;

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
        let slope = if den == 0.0 { 0.0 } else { num / den };
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

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
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
