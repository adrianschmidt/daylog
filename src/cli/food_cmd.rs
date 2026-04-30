//! `daylog food` — append a food entry to the day's `## Food` section.
//! Implementation is split across tasks: amount parsing, nutrient scaling,
//! and output formatting here; DB lookup and CLI wiring in Task 10.

use color_eyre::eyre::{bail, Result};
use color_eyre::Help;

use crate::config::Config;
use crate::db::{FoodLookup, TotalPanel};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AmountUnit {
    Gram,
    Milliliter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amount {
    pub value: f64,
    pub unit: AmountUnit,
}

impl Amount {
    pub fn unit_str(self) -> &'static str {
        match self.unit {
            AmountUnit::Gram => "g",
            AmountUnit::Milliliter => "ml",
        }
    }
}

/// Parse an amount with optional `g` / `ml` suffix. Bare numbers default
/// to grams. Whitespace between number and suffix is tolerated.
pub fn parse_amount(s: &str) -> Result<Amount> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        bail!("Invalid amount: empty.");
    }

    let lower = trimmed.to_ascii_lowercase();
    let (number_part, unit) = if let Some(rest) = lower.strip_suffix("ml") {
        (rest.trim_end(), AmountUnit::Milliliter)
    } else if let Some(rest) = lower.strip_suffix('g') {
        (rest.trim_end(), AmountUnit::Gram)
    } else {
        (lower.as_str(), AmountUnit::Gram)
    };

    let value: f64 = number_part.parse().map_err(|_| {
        color_eyre::eyre::eyre!(
            "Invalid amount: '{trimmed}'. Expected a number with optional 'g' or 'ml' suffix \
             (e.g., 500g, 250ml, or 500)."
        )
    })?;

    if value <= 0.0 {
        return Err(color_eyre::eyre::eyre!(
            "Invalid amount: '{trimmed}'. Must be positive."
        ))
        .suggestion("Pass a positive number, e.g., 500g.");
    }

    Ok(Amount { value, unit })
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedEntry {
    pub display_name: String,
    /// `(value, unit_str)` shown in the parens, or `None` to omit.
    pub amount_segment: Option<(f64, &'static str)>,
    pub kcal: Option<f64>,
    pub protein: Option<f64>,
    pub carbs: Option<f64>,
    pub fat: Option<f64>,
    pub gi: Option<f64>,
    pub gl: Option<f64>,
    pub ii: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct CustomNutrients {
    pub kcal: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub gi: Option<f64>,
    pub gl: Option<f64>,
    pub ii: Option<f64>,
}

/// Build a `RenderedEntry` from a custom-flag invocation.
pub fn render_custom(
    display_name: &str,
    amount: Option<Amount>,
    flags: CustomNutrients,
) -> RenderedEntry {
    let gl = flags.gl.or_else(|| auto_gl(flags.gi, Some(flags.carbs)));
    RenderedEntry {
        display_name: display_name.to_string(),
        amount_segment: amount.map(|a| (a.value, a.unit_str())),
        kcal: Some(flags.kcal),
        protein: Some(flags.protein),
        carbs: Some(flags.carbs),
        fat: Some(flags.fat),
        gi: flags.gi,
        gl,
        ii: flags.ii,
    }
}

/// Build a `RenderedEntry` from a nutrition-db lookup + optional amount.
/// Returns an error for invalid combinations (e.g., per_100g-only food
/// asked for in ml without a density).
pub fn render_lookup(food: &FoodLookup, amount: Option<Amount>) -> Result<RenderedEntry> {
    match amount {
        None => render_total_only(food),
        Some(a) => render_with_amount(food, a),
    }
}

fn render_total_only(food: &FoodLookup) -> Result<RenderedEntry> {
    let total = food.total.as_ref().ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "{} requires an amount (e.g., '500g' or '250ml'). It has \
             per_100g/per_100ml values but no total panel.",
            food.name
        )
    })?;
    let amount_segment = total.weight_g.map(|g| (g, "g"));
    let gi = food.gi;
    let gl = total_gl(food, total);
    Ok(RenderedEntry {
        display_name: food.name.clone(),
        amount_segment,
        kcal: total.kcal,
        protein: total.protein,
        carbs: total.carbs,
        fat: total.fat,
        gi,
        gl,
        ii: food.ii,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PanelKind {
    Per100g,
    Per100ml,
}

fn render_with_amount(food: &FoodLookup, amount: Amount) -> Result<RenderedEntry> {
    if food.per_100g.is_none() && food.per_100ml.is_none() && food.total.is_some() {
        eprintln!(
            "Warning: {} only has a `total` panel; ignoring amount {}{}.",
            food.name,
            amount.value,
            amount.unit_str()
        );
        return render_total_only(food);
    }

    // Resolve which panel to scale, what the scaling factor is, and which
    // panel kind was chosen (needed for correct GL lookup below).
    let (panel, factor, panel_kind) = match amount.unit {
        AmountUnit::Gram => match (&food.per_100g, &food.per_100ml, food.density_g_per_ml) {
            (Some(p), _, _) => (p, amount.value / 100.0, PanelKind::Per100g),
            (None, Some(p), Some(d)) if d > 0.0 => {
                // Solid input on liquid-only food via density: g → ml.
                let ml = amount.value / d;
                (p, ml / 100.0, PanelKind::Per100ml)
            }
            (None, Some(_), _) => {
                bail!(
                    "{} is a liquid (per_100ml only) and has no density_g_per_ml. \
                     Use ml: 'daylog food {} {}ml'.",
                    food.name,
                    food.name,
                    amount.value
                );
            }
            (None, None, _) => bail!(
                "{} has no per_100g/per_100ml panels and no total. Cannot scale.",
                food.name
            ),
        },
        AmountUnit::Milliliter => match (&food.per_100ml, &food.per_100g, food.density_g_per_ml) {
            (Some(p), _, _) => (p, amount.value / 100.0, PanelKind::Per100ml),
            (None, Some(p), Some(d)) if d > 0.0 => {
                // Liquid input on solid-only food via density: ml → g.
                let g = amount.value * d;
                (p, g / 100.0, PanelKind::Per100g)
            }
            (None, Some(_), _) => {
                bail!(
                    "{} is a solid (per_100g only) and has no density_g_per_ml. \
                     Use grams: 'daylog food {} {}g'.",
                    food.name,
                    food.name,
                    amount.value
                );
            }
            (None, None, _) => bail!(
                "{} has no per_100g/per_100ml panels and no total. Cannot scale.",
                food.name
            ),
        },
    };

    let kcal = panel.kcal.map(|v| v * factor);
    let protein = panel.protein.map(|v| v * factor);
    let carbs = panel.carbs.map(|v| v * factor);
    let fat = panel.fat.map(|v| v * factor);

    let gi = food.gi;
    // Key GL lookup on the panel actually chosen, not the input unit.
    // When density bridges the units (e.g., ml input → per_100g panel),
    // using the input unit would look up the wrong GL column.
    let gl_from_panel = match panel_kind {
        PanelKind::Per100g => food.gl_per_100g.map(|v| v * factor),
        PanelKind::Per100ml => food.gl_per_100ml.map(|v| v * factor),
    };
    let gl = gl_from_panel.or_else(|| auto_gl(gi, carbs));

    Ok(RenderedEntry {
        display_name: food.name.clone(),
        amount_segment: Some((amount.value, amount.unit_str())),
        kcal,
        protein,
        carbs,
        fat,
        gi,
        gl,
        ii: food.ii,
    })
}

/// GL auto-compute from GI and carbs: `gi * carbs / 100`.
fn auto_gl(gi: Option<f64>, carbs: Option<f64>) -> Option<f64> {
    match (gi, carbs) {
        (Some(g), Some(c)) => Some(g * c / 100.0),
        _ => None,
    }
}

fn total_gl(food: &FoodLookup, total: &TotalPanel) -> Option<f64> {
    food.gl_per_100g
        .and_then(|v| total.weight_g.map(|w| v * w / 100.0))
        .or_else(|| auto_gl(food.gi, total.carbs))
}

/// Format a fully-resolved entry as the `## Food` line. Caller supplies
/// the timestamp prefix (e.g., `"12:42"`).
pub fn format_line(entry: &RenderedEntry, timestamp: &str) -> String {
    let mut line = format!("- **{timestamp}** {}", entry.display_name);

    if let Some((value, unit)) = entry.amount_segment {
        line.push_str(&format!(" ({})", format_amount(value, unit)));
    }

    let nutrients = format_nutrient_segment(entry);
    if !nutrients.is_empty() {
        line.push_str(&format!(" ({nutrients})"));
    }

    let glycemic = format_glycemic_segment(entry);
    if !glycemic.is_empty() {
        line.push_str(&format!(" | {glycemic}"));
    }

    line
}

fn format_amount(value: f64, unit: &str) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}{unit}", value.round() as i64)
    } else {
        format!("{value:.1}{unit}")
    }
}

fn format_nutrient_segment(entry: &RenderedEntry) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(kcal) = entry.kcal {
        parts.push(format!("{} kcal", kcal.round() as i64));
    }
    if let Some(p) = entry.protein {
        parts.push(format!("{p:.1}g protein"));
    }
    if let Some(c) = entry.carbs {
        parts.push(format!("{c:.1}g carbs"));
    }
    if let Some(f) = entry.fat {
        parts.push(format!("{f:.1}g fat"));
    }
    parts.join(", ")
}

fn format_glycemic_segment(entry: &RenderedEntry) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(g) = entry.gi {
        parts.push(format!("GI ~{}", round_glycemic(g)));
    }
    if let Some(g) = entry.gl {
        parts.push(format!("GL ~{}", round_glycemic_one_decimal(g)));
    }
    if let Some(g) = entry.ii {
        parts.push(format!("II ~{}", round_glycemic(g)));
    }
    parts.join(", ")
}

fn round_glycemic(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn round_glycemic_one_decimal(v: f64) -> String {
    format!("{v:.1}")
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    _name: &str,
    _amount: Option<&str>,
    _kcal: Option<f64>,
    _protein: Option<f64>,
    _carbs: Option<f64>,
    _fat: Option<f64>,
    _gi: Option<f64>,
    _gl: Option<f64>,
    _ii: Option<f64>,
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    bail!("daylog food: amount parsing only — full implementation in next task")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{FoodLookup, NutrientPanel, TotalPanel};

    fn lookup_per_100g() -> FoodLookup {
        FoodLookup {
            id: 1,
            name: "Kelda Skogssvampsoppa".into(),
            per_100g: Some(NutrientPanel {
                kcal: Some(70.0),
                protein: Some(1.4),
                carbs: Some(4.8),
                fat: Some(5.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            per_100ml: None,
            density_g_per_ml: None,
            total: None,
            gi: Some(40.0),
            gl_per_100g: Some(2.0),
            gl_per_100ml: None,
            ii: Some(35.0),
            description: None,
            notes: None,
        }
    }

    fn lookup_per_100ml_with_density() -> FoodLookup {
        FoodLookup {
            id: 2,
            name: "Helmjölk".into(),
            per_100g: None,
            per_100ml: Some(NutrientPanel {
                kcal: Some(62.0),
                protein: Some(3.4),
                carbs: Some(4.8),
                fat: Some(3.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            density_g_per_ml: Some(1.03),
            total: None,
            gi: Some(30.0),
            gl_per_100g: None,
            gl_per_100ml: None,
            ii: Some(90.0),
            description: None,
            notes: None,
        }
    }

    fn lookup_total_panel() -> FoodLookup {
        FoodLookup {
            id: 3,
            name: "Te, Earl Grey, hot".into(),
            per_100g: None,
            per_100ml: None,
            density_g_per_ml: None,
            total: Some(TotalPanel {
                weight_g: Some(200.0),
                kcal: Some(2.0),
                protein: Some(0.0),
                carbs: Some(0.4),
                fat: Some(0.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            gi: None,
            gl_per_100g: None,
            gl_per_100ml: None,
            ii: None,
            description: None,
            notes: None,
        }
    }

    #[test]
    fn lookup_solid_with_grams_scales_per_100g() {
        let f = lookup_per_100g();
        let amt = parse_amount("500g").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        assert_eq!(r.kcal, Some(350.0));
        assert!((r.protein.unwrap() - 7.0).abs() < 1e-9);
        assert_eq!(r.gl, Some(10.0));
        assert_eq!(r.gi, Some(40.0));
        assert_eq!(r.amount_segment, Some((500.0, "g")));
    }

    #[test]
    fn lookup_liquid_with_ml_scales_per_100ml() {
        let f = lookup_per_100ml_with_density();
        let amt = parse_amount("250ml").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        assert_eq!(r.kcal, Some(155.0));
        assert!((r.protein.unwrap() - 8.5).abs() < 1e-9);
        assert_eq!(r.amount_segment, Some((250.0, "ml")));
    }

    #[test]
    fn lookup_solid_with_ml_uses_density() {
        // Build a solid with density to allow ml input via conversion.
        let mut f = lookup_per_100g();
        f.density_g_per_ml = Some(1.0);
        let amt = parse_amount("100ml").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        // 100ml * 1.0 = 100g; same as 100g of soup.
        assert_eq!(r.kcal, Some(70.0));
        assert_eq!(r.amount_segment, Some((100.0, "ml")));
    }

    #[test]
    fn lookup_solid_with_ml_no_density_errors() {
        let f = lookup_per_100g();
        let amt = parse_amount("100ml").unwrap();
        let err = render_lookup(&f, Some(amt)).unwrap_err();
        assert!(err.to_string().contains("density"), "got: {err}");
    }

    #[test]
    fn lookup_total_panel_no_amount_uses_totals() {
        let f = lookup_total_panel();
        let r = render_lookup(&f, None).unwrap();
        assert_eq!(r.kcal, Some(2.0));
        assert_eq!(r.amount_segment, Some((200.0, "g")));
    }

    #[test]
    fn lookup_total_panel_no_amount_no_weight_g_omits_amount() {
        let mut f = lookup_total_panel();
        f.total.as_mut().unwrap().weight_g = None;
        let r = render_lookup(&f, None).unwrap();
        assert!(r.amount_segment.is_none());
    }

    #[test]
    fn lookup_per_100g_no_amount_errors() {
        let f = lookup_per_100g();
        let err = render_lookup(&f, None).unwrap_err();
        assert!(err.to_string().contains("requires an amount"));
    }

    #[test]
    fn custom_with_gi_carbs_no_gl_autocomputes() {
        let r = render_custom(
            "Random pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: Some(50.0),
                gl: None,
                ii: None,
            },
        );
        assert_eq!(r.gl, Some(12.0));
        assert_eq!(r.gi, Some(50.0));
    }

    #[test]
    fn format_line_full_lookup() {
        let f = lookup_per_100g();
        let r = render_lookup(&f, Some(parse_amount("500g").unwrap())).unwrap();
        let line = format_line(&r, "12:42");
        assert_eq!(
            line,
            "- **12:42** Kelda Skogssvampsoppa (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~40, GL ~10.0, II ~35"
        );
    }

    #[test]
    fn format_line_omits_glycemic_when_absent() {
        let r = render_custom(
            "Random pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: None,
                gl: None,
                ii: None,
            },
        );
        let line = format_line(&r, "13:00");
        assert!(!line.contains('|'), "got: {line}");
        assert!(line.contains("(350 kcal"));
    }

    #[test]
    fn format_line_glycemic_partial() {
        let r = render_custom(
            "Pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: Some(50.0),
                gl: None,
                ii: None,
            },
        );
        let line = format_line(&r, "13:00");
        assert!(line.contains("| GI ~50, GL ~12.0"));
        assert!(!line.contains("II"));
    }

    #[test]
    fn format_line_total_panel_no_amount_no_parens() {
        let mut f = lookup_total_panel();
        f.total.as_mut().unwrap().weight_g = None;
        let r = render_lookup(&f, None).unwrap();
        let line = format_line(&r, "14:50");
        // No `(...g)` segment when weight_g is missing.
        assert!(
            line.starts_with("- **14:50** Te, Earl Grey, hot ("),
            "expected nutrient segment to start; got: {line}"
        );
        // The opening paren after the name should be the nutrient segment.
        let after_name = line.trim_start_matches("- **14:50** Te, Earl Grey, hot ");
        assert!(after_name.starts_with("(2 kcal"), "got: {after_name}");
    }

    #[test]
    fn parse_grams_with_suffix() {
        let a = parse_amount("500g").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_ml_with_suffix() {
        let a = parse_amount("250ml").unwrap();
        assert_eq!(a.value, 250.0);
        assert_eq!(a.unit, AmountUnit::Milliliter);
    }

    #[test]
    fn parse_bare_number_defaults_to_grams() {
        let a = parse_amount("500").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_decimal_amount() {
        let a = parse_amount("12.5g").unwrap();
        assert_eq!(a.value, 12.5);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_uppercase_suffix() {
        let a = parse_amount("250ML").unwrap();
        assert_eq!(a.unit, AmountUnit::Milliliter);
    }

    #[test]
    fn parse_with_internal_space() {
        let a = parse_amount("500 g").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_garbage_errors() {
        assert!(parse_amount("500abc").is_err());
        assert!(parse_amount("abc").is_err());
        assert!(parse_amount("").is_err());
    }

    #[test]
    fn parse_negative_or_zero_errors() {
        assert!(parse_amount("-5g").is_err());
        assert!(parse_amount("0g").is_err());
    }

    #[test]
    fn lookup_density_bridge_uses_correct_gl_panel() {
        // per_100g-only food with gl_per_100g set, queried with ml input.
        // Without the fix, GL would be looked up on gl_per_100ml (None),
        // dropped, and auto-compute would only rescue if gi is set.
        // Strip gi to ensure auto-compute can't mask the bug.
        let mut f = lookup_per_100g();
        f.density_g_per_ml = Some(1.0);
        f.gi = None;
        let amt = parse_amount("200ml").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        // gl_per_100g = 2.0; 200ml * 1.0 g/ml = 200g; factor = 200/100 = 2.
        // Expected GL = 2.0 * 2.0 = 4.0.
        assert_eq!(
            r.gl,
            Some(4.0),
            "expected per_100g GL to be used in density-bridge"
        );
    }
}
