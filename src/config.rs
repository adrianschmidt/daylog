use chrono::{Local, NaiveDate, Timelike};
use color_eyre::eyre::{Result, WrapErr};
use color_eyre::Section;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeightUnit {
    #[default]
    Lbs,
    Kg,
}

impl fmt::Display for WeightUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WeightUnit::Lbs => write!(f, "lbs"),
            WeightUnit::Kg => write!(f, "kg"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub notes_dir: String,
    #[serde(default = "default_db_path")]
    pub db_path: Option<String>,
    #[serde(default = "default_refresh_secs")]
    pub refresh_secs: u64,
    #[serde(default)]
    pub day_start_hour: u8,
    /// Unit for weight display. The database stores raw numbers without unit
    /// information, so changing this mid-use makes historical values ambiguous
    /// (no automatic conversion is performed).
    #[serde(default)]
    pub weight_unit: WeightUnit,
    #[serde(default)]
    pub modules: ModulesConfig,
    #[serde(default)]
    pub exercises: HashMap<String, ExerciseConfig>,
    #[serde(default)]
    pub metrics: HashMap<String, MetricConfig>,
    #[serde(default = "default_toml_table")]
    pub climbing: toml::Value,
}

fn default_db_path() -> Option<String> {
    None
}

fn default_toml_table() -> toml::Value {
    toml::Value::Table(toml::map::Map::new())
}

fn default_refresh_secs() -> u64 {
    15
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModulesConfig {
    #[serde(default = "default_true")]
    pub dashboard: bool,
    #[serde(default = "default_true")]
    pub training: bool,
    #[serde(default = "default_true")]
    pub trends: bool,
    #[serde(default)]
    pub climbing: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExerciseConfig {
    pub display: String,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "white".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricConfig {
    pub display: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub unit: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            color_eyre::eyre::bail!(
                "Config not found at {}. Run `daylog init` to create one.",
                path.display()
            );
        }
        let contents = std::fs::read_to_string(&path)
            .wrap_err_with(|| format!("Failed to read config at {}", path.display()))?;
        let config: Config = toml::from_str(&contents).map_err(|e| {
            let err = color_eyre::eyre::eyre!("Failed to parse config at {}: {e}", path.display());
            if e.message().contains("weight_unit") {
                err.suggestion("weight_unit must be \"kg\" or \"lbs\" (default: \"lbs\").")
            } else {
                err
            }
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_or_keep(current: &Config) -> Config {
        match Self::load() {
            Ok(new_config) => new_config,
            Err(e) => {
                eprintln!("Warning: config reload failed: {e}. Keeping current config.");
                current.clone()
            }
        }
    }

    fn validate(&self) -> Result<()> {
        let notes = self.notes_dir_path();
        if !notes.exists() {
            color_eyre::eyre::bail!(
                "Notes directory does not exist: {}. Check notes_dir in your config or run `daylog init`.",
                notes.display()
            );
        }
        if !notes.is_dir() {
            color_eyre::eyre::bail!(
                "notes_dir points to a file, not a directory: {}. Check your config.",
                notes.display()
            );
        }
        if self.day_start_hour > 23 {
            return Err(color_eyre::eyre::eyre!(
                "day_start_hour must be between 0 and 23, got {}.",
                self.day_start_hour
            ))
            .suggestion("Set day_start_hour to a value between 0 and 23 in your config.toml.");
        }
        Ok(())
    }

    /// Returns today's effective date as a `NaiveDate`, shifted by `day_start_hour`.
    ///
    /// If the current time is before `day_start_hour`, the effective date
    /// is yesterday. For example, with `day_start_hour = 4`, 00:30 on
    /// April 10 counts as April 9.
    pub fn effective_today_date(&self) -> NaiveDate {
        effective_date_naive(Local::now(), self.day_start_hour)
    }

    /// Returns today's effective date as a formatted `YYYY-MM-DD` string.
    pub fn effective_today(&self) -> String {
        self.effective_today_date().format("%Y-%m-%d").to_string()
    }

    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine config directory"))?
            .join("daylog");
        Ok(dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn notes_dir_path(&self) -> PathBuf {
        expand_tilde(&self.notes_dir)
    }

    pub fn db_path(&self) -> PathBuf {
        match &self.db_path {
            Some(p) => expand_tilde(p),
            None => self.notes_dir_path().join(".daylog.db"),
        }
    }

    pub fn module_config(&self, id: &str) -> Option<&toml::Value> {
        if id == "climbing" {
            if self.climbing.is_table() {
                Some(&self.climbing)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        match id {
            "dashboard" => self.modules.dashboard,
            "training" => self.modules.training,
            "trends" => self.modules.trends,
            "climbing" => self.modules.climbing,
            _ => false,
        }
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

pub fn default_config_contents() -> &'static str {
    include_str!("../presets/default.toml")
}

#[cfg(test)]
fn effective_date<Tz: chrono::TimeZone>(now: chrono::DateTime<Tz>, day_start_hour: u8) -> String
where
    Tz::Offset: std::fmt::Display,
{
    effective_date_naive(now, day_start_hour)
        .format("%Y-%m-%d")
        .to_string()
}

/// Compute the effective `NaiveDate` for a given datetime and day-start hour.
pub(crate) fn effective_date_naive<Tz: chrono::TimeZone>(
    now: chrono::DateTime<Tz>,
    day_start_hour: u8,
) -> NaiveDate {
    debug_assert!(day_start_hour <= 23, "day_start_hour must be 0..=23");
    let date = now.date_naive();
    if (now.hour() as u8) < day_start_hour {
        date.checked_sub_days(chrono::Days::new(1))
            .expect("date subtraction should not underflow")
    } else {
        date
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/notes");
        assert!(expanded.to_str().unwrap().contains("notes"));
        assert!(!expanded.to_str().unwrap().starts_with("~"));
    }

    #[test]
    fn test_parse_default_config() {
        let config: Config = toml::from_str(default_config_contents()).unwrap();
        assert!(config.modules.dashboard);
        assert!(config.modules.training);
        assert!(config.modules.trends);
        assert!(!config.modules.climbing);
        assert_eq!(config.day_start_hour, 0);
    }

    #[test]
    fn test_parse_day_start_hour() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp/test'\nday_start_hour = 4\n[modules]\n").unwrap();
        assert_eq!(config.day_start_hour, 4);
    }

    #[test]
    fn test_day_start_hour_defaults_to_zero() {
        let config: Config = toml::from_str("notes_dir = '/tmp/test'\n[modules]\n").unwrap();
        assert_eq!(config.day_start_hour, 0);
    }

    #[test]
    fn test_day_start_hour_over_23_rejected() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp'\nday_start_hour = 24\n[modules]\n").unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let config = Config {
            notes_dir: dir.path().to_str().unwrap().to_string(),
            ..config
        };
        let err = config.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("day_start_hour"),
            "error should mention day_start_hour: {msg}"
        );
    }

    // -- effective_date tests --

    use chrono::TimeZone;

    fn local(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
    ) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    #[test]
    fn test_effective_date_midnight_boundary_default() {
        // With day_start_hour=0, 00:30 on Apr 10 → Apr 10
        let dt = local(2026, 4, 10, 0, 30);
        assert_eq!(effective_date(dt, 0), "2026-04-10");
    }

    #[test]
    fn test_effective_date_before_boundary() {
        // With day_start_hour=4, 00:30 on Apr 10 → Apr 9 (still "yesterday")
        let dt = local(2026, 4, 10, 0, 30);
        assert_eq!(effective_date(dt, 4), "2026-04-09");
    }

    #[test]
    fn test_effective_date_at_boundary() {
        // With day_start_hour=4, 04:00 on Apr 10 → Apr 10 (new day starts)
        let dt = local(2026, 4, 10, 4, 0);
        assert_eq!(effective_date(dt, 4), "2026-04-10");
    }

    #[test]
    fn test_effective_date_after_boundary() {
        // With day_start_hour=4, 23:00 on Apr 9 → Apr 9 (normal)
        let dt = local(2026, 4, 9, 23, 0);
        assert_eq!(effective_date(dt, 4), "2026-04-09");
    }

    #[test]
    fn test_effective_date_just_before_boundary() {
        // With day_start_hour=4, 03:59 on Apr 10 → Apr 9
        let dt = local(2026, 4, 10, 3, 59);
        assert_eq!(effective_date(dt, 4), "2026-04-09");
    }

    #[test]
    fn test_effective_date_day_start_hour_23() {
        // With day_start_hour=23, only 23:00-23:59 is "today".
        // Everything from 0:00-22:59 rolls back to the previous day.
        let before = local(2026, 4, 10, 22, 59);
        assert_eq!(effective_date(before, 23), "2026-04-09");

        let at = local(2026, 4, 10, 23, 0);
        assert_eq!(effective_date(at, 23), "2026-04-10");

        let midnight = local(2026, 4, 10, 0, 0);
        assert_eq!(effective_date(midnight, 23), "2026-04-09");

        let midday = local(2026, 4, 10, 12, 0);
        assert_eq!(effective_date(midday, 23), "2026-04-09");
    }

    #[test]
    fn test_effective_date_jan_1_rollback() {
        // With day_start_hour=5, 02:00 on Jan 1 → Dec 31 of previous year
        let dt = local(2026, 1, 1, 2, 0);
        assert_eq!(effective_date(dt, 5), "2025-12-31");
    }

    #[test]
    fn test_weight_unit_defaults_to_lbs() {
        let config: Config = toml::from_str("notes_dir = '/tmp/test'\n").unwrap();
        assert_eq!(config.weight_unit, WeightUnit::Lbs);
    }

    #[test]
    fn test_weight_unit_kg() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp/test'\nweight_unit = 'kg'\n").unwrap();
        assert_eq!(config.weight_unit, WeightUnit::Kg);
    }

    #[test]
    fn test_weight_unit_lbs_explicit() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp/test'\nweight_unit = 'lbs'\n").unwrap();
        assert_eq!(config.weight_unit, WeightUnit::Lbs);
    }

    #[test]
    fn test_weight_unit_invalid() {
        let result: std::result::Result<Config, _> =
            toml::from_str("notes_dir = '/tmp/test'\nweight_unit = 'stones'\n");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().message().to_string();
        assert!(
            err_msg.contains("unknown variant"),
            "error should mention unknown variant: {err_msg}"
        );
    }

    #[test]
    fn test_weight_unit_display() {
        assert_eq!(WeightUnit::Lbs.to_string(), "lbs");
        assert_eq!(WeightUnit::Kg.to_string(), "kg");
    }
}
