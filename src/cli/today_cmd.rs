//! `daylog today [date]` — print a compact daily summary.

use chrono::NaiveDate;
use color_eyre::eyre::Result;

use crate::config::{Config, WeightUnit};
use crate::food_sum::FoodTotals;
use crate::goals::{Goals, Threshold};

#[derive(Debug, Clone, Default)]
pub struct DayFields {
    pub weight: Option<f64>,
    pub sleep_hours: Option<f64>,
    pub sleep_start: Option<String>,
    pub sleep_end: Option<String>,
    pub mood: Option<i32>,
    pub energy: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct BpReading {
    pub sys: i32,
    pub dia: i32,
    pub pulse: i32,
}

/// One row in the `[metrics]` config-driven custom-metrics list.
#[derive(Debug, Clone)]
pub struct CustomMetric {
    pub id: String,
    pub display: String,
    pub value: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub food: FoodTotals,
    pub day: DayFields,
    /// `(delta, previous_logged_date)` if today has a weight and a prior
    /// day with a weight exists.
    pub weight_delta: Option<(f64, NaiveDate)>,
    pub bp_morning: Option<BpReading>,
    pub custom_metrics: Vec<CustomMetric>,
    pub goals_warnings: Vec<String>,
    pub weight_unit: WeightUnit,
}

pub fn execute(_date_flag: Option<&str>, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("daylog today: not yet implemented")
}

const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn paint(color: bool, code: &str, body: &str) -> String {
    if color {
        format!("{code}{body}{RESET}")
    } else {
        body.to_string()
    }
}

/// Render the summary as a human-readable terminal block.
/// `color = true` enables ANSI escape codes for accent colors.
pub fn render_text(summary: &DaySummary, goals: &Goals, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} — Daily summary\n\n", summary.date));

    // --- Food block ---
    let kcal_t = goals.thresholds.get("kcal");
    out.push_str(&render_food_row(
        "Calories",
        summary.food.kcal,
        "kcal",
        kcal_t,
        color,
    ));
    let protein_t = goals.thresholds.get("protein");
    out.push_str(&render_food_row(
        "Protein",
        summary.food.protein,
        "g",
        protein_t,
        color,
    ));
    let carbs_t = goals.thresholds.get("carbs");
    out.push_str(&render_food_row(
        "Carbs",
        summary.food.carbs,
        "g",
        carbs_t,
        color,
    ));
    let fat_t = goals.thresholds.get("fat");
    out.push_str(&render_food_row("Fat", summary.food.fat, "g", fat_t, color));

    out.push('\n');

    // --- Weight / Sleep / BP ---
    out.push_str(&render_weight_row(
        summary,
        goals.thresholds.get("weight"),
        color,
    ));
    out.push_str(&render_sleep_row(summary, color));
    out.push_str(&render_bp_row(summary, color));

    // --- Custom metrics ---
    for m in &summary.custom_metrics {
        out.push_str(&render_custom_row(m, goals.thresholds.get(&m.id), color));
    }

    // --- Hint lines ---
    let mut hints: Vec<String> = Vec::new();
    if !goals.present {
        hints.push(format!(
            "(No goals defined — add `<metric>_min/_max/_target` keys to {}.)",
            goals.source_path.display()
        ));
    }
    if summary.food.skipped_lines > 0 {
        let plural = if summary.food.skipped_lines == 1 {
            ""
        } else {
            "s"
        };
        hints.push(format!(
            "({} food line{plural} couldn't be parsed)",
            summary.food.skipped_lines
        ));
    }
    for w in &summary.goals_warnings {
        hints.push(format!("({w})"));
    }
    if !hints.is_empty() {
        out.push('\n');
        for h in hints {
            out.push_str(&paint(color, DIM, &h));
            out.push('\n');
        }
    }

    out
}

fn render_food_row(
    label: &str,
    value: f64,
    unit: &str,
    threshold: Option<&Threshold>,
    color: bool,
) -> String {
    let value_int = value.round() as i64;
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, unit),
        None => String::new(),
    };
    let annotation = match threshold {
        Some(t) => annotate_value(value, t, color),
        None => String::new(),
    };
    let body = if goal_part.is_empty() {
        format!("{label}: {value_int} {unit}")
    } else {
        format!("{label}: {value_int} / {goal_part}")
    };
    if annotation.is_empty() {
        format!("{body}\n")
    } else {
        format!("{body}     {annotation}\n")
    }
}

/// Format a threshold inline: "1900–2200 kcal", "≥140 g", "≤65 bpm",
/// "→ 110 kg", or combinations.
fn format_threshold_inline(t: &Threshold, unit: &str) -> String {
    match (t.min, t.max, t.target) {
        (Some(min), Some(max), _) => format!("{}–{} {unit}", trim_num(min), trim_num(max)),
        (Some(min), None, _) => format!("≥{} {unit}", trim_num(min)),
        (None, Some(max), _) => format!("≤{} {unit}", trim_num(max)),
        (None, None, Some(tgt)) => format!("→ {} {unit}", trim_num(tgt)),
        (None, None, None) => String::new(),
    }
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

/// Build the trailing `(387 below min)` / `✓ over minimum` / `✓ within range`
/// annotation for a value vs threshold.
fn annotate_value(value: f64, t: &Threshold, color: bool) -> String {
    if let Some(min) = t.min {
        if value < min {
            let delta = (min - value).round() as i64;
            return paint(color, RED, &format!("({delta} below min)"));
        }
    }
    if let Some(max) = t.max {
        if value > max {
            let delta = (value - max).round() as i64;
            return paint(color, RED, &format!("({delta} above max)"));
        }
    }
    if t.min.is_some() && t.max.is_none() {
        return paint(color, GREEN, "✓ over minimum");
    }
    if t.min.is_none() && t.max.is_some() {
        return paint(color, GREEN, "✓ under maximum");
    }
    if t.min.is_some() && t.max.is_some() {
        return paint(color, GREEN, "✓ within range");
    }
    // Target-only: don't annotate (just show the target inline).
    String::new()
}

fn render_weight_row(summary: &DaySummary, threshold: Option<&Threshold>, color: bool) -> String {
    let unit = summary.weight_unit.to_string();
    let value = match summary.day.weight {
        Some(v) => v,
        None => {
            return format!("Weight:    {}\n", paint(color, DIM, "not logged"));
        }
    };
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, &unit),
        None => String::new(),
    };
    let mut line = if goal_part.is_empty() {
        format!("Weight:    {} {unit}", trim_num(value))
    } else {
        format!("Weight:    {} {unit} / {goal_part}", trim_num(value))
    };
    if let Some((delta, prev_date)) = summary.weight_delta {
        let label = format_delta_label(summary.date, prev_date);
        let sign = if delta >= 0.0 { "+" } else { "" };
        line.push_str(&format!("  (Δ {sign}{} vs {label})", trim_num(delta)));
    }
    line.push('\n');
    line
}

fn format_delta_label(today: NaiveDate, prev: NaiveDate) -> String {
    let diff = today.signed_duration_since(prev).num_days();
    if diff == 1 {
        "yesterday".into()
    } else {
        prev.format("%Y-%m-%d").to_string()
    }
}

fn render_sleep_row(summary: &DaySummary, color: bool) -> String {
    match summary.day.sleep_hours {
        Some(h) => {
            let hours = h.floor() as i64;
            let mins = ((h - h.floor()) * 60.0).round() as i64;
            format!("Sleep:     {hours}h {mins:02}min\n")
        }
        None => format!("Sleep:     {}\n", paint(color, DIM, "not logged")),
    }
}

fn render_bp_row(summary: &DaySummary, color: bool) -> String {
    match &summary.bp_morning {
        Some(b) => format!("BP morning:   {}/{} (pulse {})\n", b.sys, b.dia, b.pulse),
        None => format!("BP morning:   {}\n", paint(color, DIM, "not logged")),
    }
}

fn render_custom_row(metric: &CustomMetric, threshold: Option<&Threshold>, color: bool) -> String {
    let unit_str = metric.unit.as_deref().unwrap_or("");
    let value_str = match metric.value {
        Some(v) => trim_num(v),
        None => return format!("{}: {}\n", metric.display, paint(color, DIM, "not logged")),
    };
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, unit_str),
        None => String::new(),
    };
    let annotation = match (metric.value, threshold) {
        (Some(v), Some(t)) => annotate_value(v, t, color),
        _ => String::new(),
    };
    let body = if goal_part.is_empty() {
        if unit_str.is_empty() {
            format!("{}: {value_str}", metric.display)
        } else {
            format!("{}: {value_str} {unit_str}", metric.display)
        }
    } else {
        format!("{}: {value_str} / {goal_part}", metric.display)
    };
    if annotation.is_empty() {
        format!("{body}\n")
    } else {
        format!("{body}     {annotation}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn fixture_summary() -> DaySummary {
        DaySummary {
            date: NaiveDate::from_ymd_opt(2026, 4, 30).unwrap(),
            food: FoodTotals {
                kcal: 1513.0,
                protein: 147.0,
                carbs: 77.0,
                fat: 59.0,
                entry_count: 4,
                skipped_lines: 0,
            },
            day: DayFields {
                weight: Some(121.5),
                sleep_hours: Some(6.4),
                sleep_start: Some("23:00".into()),
                sleep_end: Some("05:24".into()),
                mood: None,
                energy: None,
            },
            weight_delta: Some((1.3, NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())),
            bp_morning: None,
            custom_metrics: vec![],
            goals_warnings: vec![],
            weight_unit: WeightUnit::Kg,
        }
    }

    fn fixture_goals() -> Goals {
        let mut thresholds = HashMap::new();
        thresholds.insert(
            "kcal".into(),
            Threshold {
                min: Some(1900.0),
                max: Some(2200.0),
                target: None,
            },
        );
        thresholds.insert(
            "protein".into(),
            Threshold {
                min: Some(140.0),
                max: None,
                target: None,
            },
        );
        thresholds.insert(
            "weight".into(),
            Threshold {
                target: Some(110.0),
                min: None,
                max: None,
            },
        );
        Goals {
            thresholds,
            source_path: std::path::PathBuf::from("/tmp/goals.md"),
            present: true,
        }
    }

    #[test]
    fn render_text_food_block_with_goals() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("2026-04-30 — Daily summary"), "got:\n{out}");
        assert!(out.contains("Calories:"), "got:\n{out}");
        assert!(out.contains("1513"), "got:\n{out}");
        assert!(out.contains("1900–2200 kcal"), "got:\n{out}");
        assert!(out.contains("387 below min"), "got:\n{out}");
        assert!(out.contains("Protein:"), "got:\n{out}");
        assert!(out.contains("147"), "got:\n{out}");
        assert!(out.contains("≥140 g"), "got:\n{out}");
        assert!(out.contains("over minimum"), "got:\n{out}");
        assert!(out.contains("Carbs:"), "got:\n{out}");
        assert!(out.contains("77 g"), "got:\n{out}");
        assert!(out.contains("Fat:"), "got:\n{out}");
        assert!(out.contains("59 g"), "got:\n{out}");
    }

    #[test]
    fn render_text_weight_sleep_bp_block() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("Weight:    121.5 kg"), "got:\n{out}");
        assert!(out.contains("→ 110 kg"), "got:\n{out}");
        assert!(out.contains("Δ +1.3 vs yesterday"), "got:\n{out}");
        assert!(out.contains("Sleep:     6h 24min"), "got:\n{out}");
        assert!(out.contains("BP morning:"), "got:\n{out}");
        assert!(out.contains("not logged"), "got:\n{out}");
    }

    #[test]
    fn render_text_no_goals_emits_hint() {
        let s = fixture_summary();
        let g = Goals {
            thresholds: HashMap::new(),
            source_path: std::path::PathBuf::from("/notes/goals.md"),
            present: false,
        };
        let out = render_text(&s, &g, false);
        assert!(out.contains("No goals defined"), "got:\n{out}");
        assert!(out.contains("/notes/goals.md"), "got:\n{out}");
        // No goal annotations on rows.
        assert!(!out.contains("below min"));
        assert!(!out.contains("over minimum"));
    }

    #[test]
    fn render_text_skipped_food_lines_emits_hint() {
        let mut s = fixture_summary();
        s.food.skipped_lines = 2;
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(
            out.contains("2 food lines couldn't be parsed"),
            "got:\n{out}"
        );
    }

    #[test]
    fn render_text_unknown_metric_warning() {
        let mut s = fixture_summary();
        s.goals_warnings
            .push("unknown metric `mystery` in goals.md".into());
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("unknown metric `mystery`"), "got:\n{out}");
    }

    #[test]
    fn render_text_weight_delta_non_yesterday_uses_actual_date() {
        let mut s = fixture_summary();
        s.date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        s.weight_delta = Some((0.4, NaiveDate::from_ymd_opt(2026, 4, 25).unwrap()));
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("Δ +0.4 vs 2026-04-25"), "got:\n{out}");
        assert!(!out.contains("vs yesterday"));
    }

    #[test]
    fn render_text_color_off_strips_escapes() {
        let mut s = fixture_summary();
        s.day.weight = None; // forces a "not logged" row
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(!out.contains("\x1b["), "got:\n{out:?}");
    }

    #[test]
    fn render_text_color_on_includes_escapes_for_below_min() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, true);
        assert!(out.contains("\x1b[31m"), "got:\n{out:?}");
    }

    #[test]
    fn render_text_custom_metric_with_max_above_max() {
        let mut s = fixture_summary();
        s.custom_metrics.push(CustomMetric {
            id: "resting_hr".into(),
            display: "Resting HR".into(),
            value: Some(72.0),
            unit: Some("bpm".into()),
        });
        let mut g = fixture_goals();
        g.thresholds.insert(
            "resting_hr".into(),
            Threshold {
                max: Some(65.0),
                min: None,
                target: None,
            },
        );
        let out = render_text(&s, &g, false);
        assert!(out.contains("Resting HR: 72 / ≤65 bpm"), "got:\n{out}");
        assert!(out.contains("7 above max"), "got:\n{out}");
    }

    #[test]
    fn render_text_target_only_threshold_has_no_annotation() {
        let mut s = fixture_summary();
        s.custom_metrics.push(CustomMetric {
            id: "rhr".into(),
            display: "RHR".into(),
            value: Some(60.0),
            unit: Some("bpm".into()),
        });
        let mut g = fixture_goals();
        g.thresholds.insert(
            "rhr".into(),
            Threshold {
                target: Some(58.0),
                min: None,
                max: None,
            },
        );
        let out = render_text(&s, &g, false);
        assert!(out.contains("RHR: 60 / → 58 bpm"), "got:\n{out}");
        // No annotation suffix for target-only thresholds: isolate the RHR
        // row and confirm it has no ✓ / below min / above max marker.
        let rhr_line = out
            .lines()
            .find(|l| l.starts_with("RHR:"))
            .expect("RHR row missing");
        assert!(!rhr_line.contains("✓"), "got:\n{rhr_line}");
        assert!(!rhr_line.contains("below min"), "got:\n{rhr_line}");
        assert!(!rhr_line.contains("above max"), "got:\n{rhr_line}");
    }
}
