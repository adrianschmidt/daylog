//! `daylog food` — append a food entry to the day's `## Food` section.
//! Implementation lands in subsequent tasks.

use color_eyre::eyre::Result;

use crate::config::Config;

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
    color_eyre::eyre::bail!("daylog food: not yet implemented")
}
