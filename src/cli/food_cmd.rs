//! `daylog food` — append a food entry to the day's `## Food` section.
//! Implementation is split across tasks: amount parsing here; nutrition
//! scaling, output formatting, and DB lookup in subsequent tasks.

use color_eyre::eyre::{bail, Result};
use color_eyre::Help;

use crate::config::Config;

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
}
